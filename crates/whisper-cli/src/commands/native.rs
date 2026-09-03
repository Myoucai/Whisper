//! native target — install-time hardware specialization with a content-
//! addressed cache (the CUDA-PTX model, minus the hand-written backend).
//!
//! Pipeline: source → import inline → self-hosted compiler on the Rust VM
//! → C → host `cc -O3 -march=native -flto` → cached executable.
//! The cache key covers everything that can change the output: expanded
//! source, compiler.all.ws, runtime.c, the cc name and the march flags —
//! so a CPU upgrade or any compiler edit misses the cache and recompiles,
//! while an unchanged program on the same machine is an instant copy.
//!
//! `WHISPER_NATIVE_MARCH` overrides -march (e.g. `x86-64` for a generic
//! build); `WHISPER_CC` overrides the host compiler, as in the c2 target.

use std::path::{Path, PathBuf};

/// Directory for the native-specialization cache.
fn cache_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".cache").join("whisper").join("native")
}

/// 128-bit content fingerprint (two seeded SipHash passes + lengths) —
/// cache keying only, not a security primitive.
fn fingerprint(parts: &[&[u8]]) -> String {
    use std::hash::{Hash, Hasher};
    let mut out = String::new();
    for seed in [0x7714550e_11d_u64, 0x9e3779b97f4a7c15] {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        seed.hash(&mut h);
        for p in parts {
            h.write(p);
            p.len().hash(&mut h);
        }
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

/// Compile `source` to a host-specialized native executable, via the
/// self-hosted chain, with a content-addressed build cache.
pub fn build_native(
    source: &str,
    source_dir: &Path,
    output: &str,
) -> Result<(), String> {
    // 1. Inline imports (auto std/core, same policy as the c2 target).
    let with_core = if source.contains("std/core") {
        source.to_string()
    } else {
        format!("import \"std/core\"\n{source}")
    };
    let expanded = super::c2::expand_imports(&with_core, source_dir)?;

    // 2. Cache key: expanded source + compiler + runtime + cc + march.
    let cc = std::env::var("WHISPER_CC").unwrap_or_else(|_| "gcc".into());
    let march = std::env::var("WHISPER_NATIVE_MARCH").unwrap_or_else(|_| "native".into());
    let whisperc_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../whisperc");
    let compiler_ws = std::fs::read(whisperc_dir.join("compiler.all.ws"))
        .map_err(|e| format!("read compiler.all.ws: {e}"))?;
    let runtime_c = std::fs::read(whisperc_dir.join("runtime.c"))
        .map_err(|e| format!("read runtime.c: {e}"))?;
    let key = fingerprint(&[
        expanded.as_bytes(),
        &compiler_ws,
        &runtime_c,
        cc.as_bytes(),
        march.as_bytes(),
        b"-O3-flto-fno-strict-aliasing-v1",
    ]);

    let dir = cache_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create cache dir: {e}"))?;
    let cached_exe = dir.join(format!("{key}.exe"));
    let cached_c = dir.join(format!("{key}.c"));

    if cached_exe.exists() {
        std::fs::copy(&cached_exe, output)
            .map_err(|e| format!("cache copy -> {output}: {e}"))?;
        println!(
            "cache hit ({key:.12}…): host-specialized binary -> {output}"
        );
        return Ok(());
    }

    // 3. Miss: run the self-hosted compiler, emit C, compile specialized.
    let c_code = super::c2::compile_via_whisperc_vm(&expanded)?;
    std::fs::write(&cached_c, &c_code)
        .map_err(|e| format!("write {}: {e}", cached_c.display()))?;

    let status = std::process::Command::new(&cc)
        .arg("-O3")
        .arg("-flto")
        .arg(format!("-march={march}"))
        .arg("-fno-strict-aliasing")
        .arg("-w")
        .arg("-o")
        .arg(&cached_exe)
        .arg(&cached_c)
        .arg("-I")
        .arg(&whisperc_dir)
        .arg("-lm")
        .args(if cfg!(windows) {
            vec!["-lws2_32"]
        } else {
            vec![]
        })
        .status()
        .map_err(|e| format!("failed to invoke {cc}: {e} (install gcc or set WHISPER_CC)"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&cached_exe);
        return Err(format!(
            "{cc} failed with {status} (C kept at {})",
            cached_c.display()
        ));
    }

    std::fs::copy(&cached_exe, output)
        .map_err(|e| format!("copy -> {output}: {e}"))?;
    println!(
        "host-specialized (-O3 -march={march} -flto) via self-hosted chain -> {output} \
         (cached as {key:.12}…)"
    );
    Ok(())
}
