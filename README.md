# Socks5 Forwarder

Forward incoming connections to socks5 proxy.

Useful for applications that not support socks5.

Also, you can use it without any proxy, and it will be a simple TCP proxy.

## How to Use
Copy and modify `docker-compose.yml`, then `docker-compose up -d`.

## Advanced Usage
For better performance I implemented a proxy with eBPF.

The kernel space and user space code is in `probe` and `userspace`.

If you start a container with ebpf, you may want to let it be privileged(in docker-compose, `privileged: true`).

## Images List
Full list -> https://hub.docker.com/repository/docker/ihciah/socks5-forwarder/tags

- generic + amd64: ihciah/socks5-forwarder:generic
- generic + aarch64: ihciah/socks5-forwarder:generic-aarch64
- ebpf + amd64: ihciah/socks5-forwarder:ebpf
- ebpf + aarch64: ihciah/socks5-forwarder:ebpf-aarch64
