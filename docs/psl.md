# Public Suffix List (PSL)

## 数据

- 源文件：`security/public_suffix_list.dat`
- 快照日期：2026-11-02
- 许可证：MPL-2.0，见 [security/PSL-LICENSE.txt](../security/PSL-LICENSE.txt)
- 生成文件：`security/psl_data.h`

## 生成

```bash
python3 scripts/gen-psl.py
```

生成器：

- 跳过注释/空行
- 分离 normal / wildcard (`*.`) / exception (`!`)
- 跳过非 ASCII 规则（native URL parser 不做 IDNA）
- 输出排序数组，运行时二分查找

## 运行时算法

`w_public_suffix(domain)`：

1. 小写化，限制 255 字节
2. exception 精确匹配 → 不是 public suffix
3. normal 精确匹配 → 是
4. wildcard：取 domain 第一个点后的部分，若在 wildcard 表 → 是
5. 否则不是

示例：

```text
co.uk             public
example.co.uk     not public
github.io         public
foo.github.io     not public
www.ck            not public (exception)
foo.ck            public (wildcard)
city.kawasaki.jp  not public (exception)
```

## 验证

```bash
bash scripts/psl-gate.sh
```

CI 也在 selfhost job 中运行。

## 更新流程

1. 下载最新 PSL 到 `security/public_suffix_list.dat`
2. `python3 scripts/gen-psl.py`
3. `bash scripts/psl-gate.sh`
4. 更新 `security/PSL-LICENSE.txt` 与快照日期
5. 提交 `public_suffix_list.dat` + `psl_data.h`

## 限制

- Unicode 规则被跳过；host 必须已是 ASCII/punycode
- PSL 是公共后缀判断，不是完整 cookie 规范
- 数据更新是手工流程，不是实时订阅
- native cache key 已包含 `psl_data.h`，更新会失效旧缓存
