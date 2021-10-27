# Socks5 Forwarder

Forward incoming connections to socks5 proxy.

Useful for applications that not support socks5.

## How to Use
Copy and modify `docker-compose.yml`, then `docker-compose up -d`.

## Advanced Usage
For better performance I implemented a proxy with eBPF.

The kernel space and user space code is in `probe` and `userspace`.
