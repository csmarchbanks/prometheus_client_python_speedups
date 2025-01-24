# Prometheus Client Python Speedups

This repo contains code meant to speed up the [Prometheus
client_python](https://github.com/prometheus/client_python/), specifically the
multiprocess mode. It is designed to be used optionally by that client for
users that are ok with including non-native code, and struggle with performance
when there are a lot of .db files present in multiprocess mode.

This repository is not meant to be used outside of client_python, and may
change signatures at any time as the disk file format used by client_python is
not guaranteed between versions.
