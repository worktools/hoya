# 构建阶段
FROM rust:1.86.0 AS builder

WORKDIR /app

# 复制依赖文件和新的锁文件
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY examples ./examples

# 构建项目
RUN cargo build --release

# 运行阶段
FROM debian:bookworm-slim

# 安装运行时依赖
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# 复制二进制文件
COPY --from=builder /app/target/release/hoya /usr/local/bin/hoya

# 创建非 root 用户
RUN useradd -m -u 1000 hoya
USER hoya

# 暴露端口（根据你的应用需要调整）
EXPOSE 8080

# 启动命令
CMD ["hoya"]