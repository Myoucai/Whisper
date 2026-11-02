# Whisper 1.3.20 发布说明

## 主题

**嵌入完整 Public Suffix List，替换保守后缀表。**

## 新增

- `security/public_suffix_list.dat`（2026-11-02 快照，MPL-2.0）
- `security/psl_data.h`：生成的有序数组
  - 9617 normal
  - 312 wildcard
  - 8 exception
- `scripts/gen-psl.py`
- `scripts/psl-gate.sh`
- `docs/psl.md`
- native cache key 纳入 `psl_data.h`

## 运行时

`w_public_suffix(domain)`：

1. 小写化，限制 255 字节
2. exception 精确匹配 → 不是 public suffix
3. normal 精确匹配 → 是
4. wildcard 取第一个点后的部分 → 是
5. 否则不是

示例：

```text
co.uk             public
example.co.uk     not public
github.io         public
foo.github.io     not public
www.ck            not public (exception)
foo.ck            public (wildcard)
```

## 验证

```text
psl-gate: PASS
http6-gate: PASS
http-parity-gate: PASS
net-allow-gate: PASS
integration tests: 123/123
```

## 兼容性

- 正常 cookie Domain 行为不变
- 现在会拒绝完整 PSL 中的 public suffix
- 非 ASCII 规则跳过；host 应是 ASCII/punycode
- native 缓存会因 PSL 更新失效并重建

## 已知限制

- PSL 是手工更新快照，不是实时订阅
- 跳过 Unicode 规则
- 不替代完整 cookie 规范
- PSL 数据遵循 MPL-2.0，见 `security/PSL-LICENSE.txt`

## 下一步

- 覆盖引导 fuzz / nightly
- 外部安全审计
- 容器/EROFS fallback
- 真实集群 cgroup 验证
