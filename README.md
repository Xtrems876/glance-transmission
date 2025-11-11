# glance-transmission

A small Glance extension that queries a Transmission RPC and returns a compact HTML widget with basic upload/download/torrent counts.

## Usage

- Run the container and point Glance to the extension URL. The extension exposes a single endpoint:

- GET /transmission?url=${TRANSMISSION_URL}/transmission/rpc

Example Glance config (do not put credentials in the URL):

```yml
- type: extension
  url: https://${GLANCE_TRANSMISSION_URL}/transmission?url=https://${TRANSMISSION_URL}/transmission/rpc
  allow-potentially-dangerous-html: true
  cache: 30s
  headers:
    X-Transmission-Username: ${TRANSMISSION_USER}
    X-Transmission-Password: ${TRANSMISSION_PASSWORD}
```