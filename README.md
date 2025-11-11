# glance-transmission

A small Glance extension that queries a Transmission RPC and returns a compact HTML widget with basic upload/download/torrent counts.

## Usage

- Run the container and point Glance to the extension URL. The extension exposes a single endpoint:

- GET /transmission?url=<transmission_base_url>

Example Glance config (do not put credentials in the URL):

```yml
- type: extension
  url: http://<glance-transmission-host>:8080/transmission?url=http://<transmission-host>:9091/transmission/rpc
  allow-potentially-dangerous-html: true
  headers:
    X-Transmission-Username: <user>
    X-Transmission-Password: <pass>
```

Notes:
- The extension returns widget headers (`Widget-Title`, `Widget-Content-Type`) so Glance can display it as an HTML widget.
- Supplying `username` and `password` is optional — only required if your Transmission RPC is protected. Passing credentials in query string is simple but not secure for production; consider using a proxy or environment-based secrets for production.