# Kubernetes 配置

这个目录包含了在 Kubernetes 中运行 hoya 应用所需的配置文件。

## 文件说明

- `deployment.yaml` - Deployment 配置，包含 3 个副本
- `service.yaml` - Service 配置，暴露应用服务
- `configmap.yaml` - 配置映射，用于环境变量管理

## 使用方法

### 1. 构建 Docker 镜像

```bash
docker build -t hoya:latest .
```

### 2. 应用到 Kubernetes 集群

```bash
# 应用所有配置
kubectl apply -f k8s/

# 或者分别应用
kubectl apply -f k8s/configmap.yaml
kubectl apply -f k8s/deployment.yaml
kubectl apply -f k8s/service.yaml
```

### 3. 检查状态

```bash
# 查看 Pod 状态
kubectl get pods -l app=hoya

# 查看 Service
kubectl get service hoya-service

# 查看日志
kubectl logs -l app=hoya
```

### 4. 访问应用

```bash
# 端口转发到本地
kubectl port-forward service/hoya-service 3000:80

# 然后访问 http://localhost:3000
# 注意：应用监听在 3000 端口，Service 将 80 端口映射到 Pod 的 3000 端口
```

## 配置说明

- 使用 ConfigMap 管理环境变量
- 配置了健康检查和就绪检查
- 设置了资源限制和请求
- 使用 ClusterIP 类型的 Service

## 扩展

可以根据需要修改：

- 副本数量（replicas）
- 资源限制（resources）
- 环境变量（ConfigMap）
- Service 类型（可以改为 NodePort 或 LoadBalancer）
