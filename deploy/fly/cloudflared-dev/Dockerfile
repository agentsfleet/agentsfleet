FROM cloudflare/cloudflared:latest

# config.yml defines ingress rules (hostname → internal service).
# cloudflared reads TUNNEL_TOKEN env var natively — no --token flag, no shell needed.
# Fly secret: fly secrets set TUNNEL_TOKEN=<token> --app cloudflared-dev
COPY config.yml /etc/cloudflared/config.yml

CMD ["tunnel", "--config", "/etc/cloudflared/config.yml", "run"]
