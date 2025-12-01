# HTTPS 自动续期配置（方案 3）

这个目录包含基于 K3s 内置 Traefik 和 cert-manager 的 HTTPS 自动续期附加配置。

## 说明

这是**附加配置**，需要配合上级目录的基础服务配置一起使用：

- 基础服务配置：`../` 目录（deployment.yaml, service.yaml, configmap.yaml）
- HTTPS 配置：本目录（cluster-issuer.yaml, ingress.yaml, nodeport-service.yaml, https-nodeport-service.yaml）

## 前提条件

1. **K3s 集群** - 已预装 Traefik Ingress Controller
2. **cert-manager** - 需要单独安装
3. **基础服务** - 已部署基础服务配置

## 快速开始

### 1. 安装 cert-manager

```bash
# 安装cert-manager
kubectl apply -f https://github.com/cert-manager/cert-manager/releases/download/v1.13.3/cert-manager.yaml

# 等待cert-manager启动
kubectl wait --for=condition=ready pod -l app=cert-manager -n cert-manager --timeout=120s
```

### 2. 配置 ClusterIssuer

编辑 `cluster-issuer.yaml`，替换邮箱地址：

```yaml
email: your-email@example.com # 替换为你的邮箱
```

### 3. 应用配置

````bash
# 1. 首先确保基础服务已部署
cd ..
kubectl apply -f configmap.yaml
kubectl apply -f deployment.yaml
kubectl apply -f service.yaml
```bash
# 2. 然后应用HTTPS配置
cd https
kubectl apply -f cluster-issuer.yaml
kubectl apply -f nodeport-service.yaml
kubectl apply -f https-nodeport-service.yaml
kubectl apply -f ingress.yaml

# 或者一键应用HTTPS配置
kubectl apply -f .

### 防火墙配置
应用配置后，需要在服务器防火墙开放以下端口：
- **30080/TCP**: HTTP 访问端口
- **30443/TCP**: HTTPS 访问端口

详见 [FIREWALL_SETUP.md](FIREWALL_SETUP.md)

### 验证配置
配置完成后，可以通过以下方式测试：
- **HTTP**: `http://hoya.tiye.me:30080/ready`
- **HTTPS**: `https://hoya.tiye.me:30443/ready`
- **证书状态**: `kubectl get certificate hoya-tls-cert --context hk`
````

### 4. 验证部署

```bash
# 检查证书状态
kubectl get certificate -A

# 检查Ingress状态
kubectl get ingress hoya-ingress

# 测试HTTPS访问
curl -I https://your-domain.com
```

## 配置文件说明

| 文件                          | 作用                       | 是否必需 |
| ----------------------------- | -------------------------- | -------- |
| `cluster-issuer.yaml`         | Let's Encrypt 证书颁发机构 | ✅       |
| `nodeport-service.yaml`       | HTTP NodePort 服务         | ✅       |
| `https-nodeport-service.yaml` | HTTPS NodePort 服务        | ✅       |
| `ingress.yaml`                | 主 Ingress 配置（HTTPS）   | ✅       |

**注意：** 基础服务配置（configmap.yaml, deployment.yaml, service.yaml）需要在上级目录`../`中先部署

## 自动续期

cert-manager 会自动处理证书续期：

- 证书有效期：90 天
- 自动续期时间：30 天前
- 无需人工干预

## 域名配置

确保你的域名已正确指向 K3s 集群的 IP 地址。

## 故障排查

```bash
# 查看cert-manager日志
kubectl logs -n cert-manager deployment/cert-manager

# 查看证书申请状态
kubectl describe certificate hoya-tls-cert

# 查看Challenge状态
kubectl describe challenge -A
```
