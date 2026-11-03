# Whisper 1.3.24 发布说明

## 主题

**Docker 容器内只读卷 fallback 真实验证。**

## 验证环境

- Docker 26.1.3
- 镜像：`python:3.11-slim`
- 容器：

```text
--network none
--read-only
--cap-drop ALL
--security-opt no-new-privileges
--user 10001:10001
-v /opt/whisper:/opt/whisper:ro
-v /srv/tenant:/srv/tenant:ro
```

## 结果

```text
wrun: preopen /srv/tenant/data is already on a read-only mount;
      using direct ro mapping
DOCKER_RC=0
```

说明：

- 容器内没有创建 userns/mount
- wrun 从 `/proc/self/mountinfo` 识别 `:ro` 卷
- 用 `/bin/true` 替代 WAMR，隔离 mount 行为
- WAMR guest 的 EROFS 写入已在 1.3.13 / OpenCloudOS 验证

## 文档

- `deploy/README.md`
- `docs/wasm-wasi-plan.md`
- `docs/security-audit.md`

## 兼容性

- 无运行时行为变化
- 仅新增真实容器验证记录

## 已知限制

- `python:3.11-slim` 是测试镜像，不是生产镜像
- 未在 Kubernetes/Podman 验证
- 无 userns 且宿主 rw 时仍 fail closed
- 容器 seccomp 可能影响 userns；已有 ro 卷不受影响
