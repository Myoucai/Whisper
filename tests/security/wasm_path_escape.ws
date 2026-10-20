// WASI security fixture: paths outside the preopen must not resolve.
// Run with --map-dir=/data::<host-dir>; both lengths must be 0.
"/data/../secret.txt" read-file strlen .
"/etc/passwd" read-file strlen .
