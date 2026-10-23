# The bastion for `cargo test -p engine --features ssh-tests`. Every key in this directory is a
# test-only fixture: never trust or reuse one outside this container.
FROM alpine:3.20
RUN apk add --no-cache openssh-server \
 && adduser -D -s /sbin/nologin tunnel \
 && echo 'tunnel:tunnel' | chpasswd
COPY sshd_config /etc/ssh/sshd_config
COPY ssh_host_ed25519_key /etc/ssh/ssh_host_ed25519_key
COPY authorized_keys /home/tunnel/.ssh/authorized_keys
# Git does not keep 0600, and sshd refuses a host key or authorized_keys others can read.
RUN chmod 600 /etc/ssh/ssh_host_ed25519_key /home/tunnel/.ssh/authorized_keys \
 && chmod 700 /home/tunnel/.ssh \
 && chown -R tunnel:tunnel /home/tunnel/.ssh
EXPOSE 22
CMD ["/usr/sbin/sshd", "-D", "-e"]
