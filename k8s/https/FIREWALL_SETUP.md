# 防火墙配置指南

## 需要开放的端口

### K3s Traefik 入口 (必须)
- **30080/TCP**: HTTP 服务端口 (NodePort)
- **30443/TCP**: HTTPS 服务端口 (NodePort)

### K3s API (可选)
- **6443/TCP**: K3s API 服务器

## 防火墙配置命令

### Ubuntu/Debian (ufw)
```bash
sudo ufw allow 30080/tcp
sudo ufw allow 30443/tcp
sudo ufw allow 6443/tcp  # 可选
```

### CentOS/RHEL (firewalld)
```bash
sudo firewall-cmd --permanent --add-port=30080/tcp
sudo firewall-cmd --permanent --add-port=30443/tcp
sudo firewall-cmd --permanent --add-port=6443/tcp  # 可选
sudo firewall-cmd --reload
```

### 云服务器安全组配置
在阿里云/腾讯云/华为云等控制台，需要在安全组中添加：
- 入站规则：30080/TCP (HTTP)
- 入站规则：30443/TCP (HTTPS)

## 验证端口开放

配置完成后，可以通过以下方式验证：

```bash
# 测试 HTTP
 curl -v http://your-server-ip:30080/ready

# 测试 HTTPS (证书配置完成后)
 curl -v -k https://your-server-ip:30443/ready
```

## 服务端口映射

当前服务配置：
- HTTP: 30080 → 80 → 3000 (Pod)
- HTTPS: 30443 → 443 → 3000 (Pod)