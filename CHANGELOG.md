# Changelog

## 0.1.0 (2026-06-22)


### Features

* **app:** pure state machine for filter/select/actions (T6) ([b61a74f](https://github.com/bfirestone/jj-workspace/commit/b61a74fe23aa1fa5010ea4b3727996e94bd9e398))
* **config:** load optional TOML config with defaults (T5) ([b555793](https://github.com/bfirestone/jj-workspace/commit/b5557931a592f9ba6dcc6e17bc88bf39c8fa39cc))
* **directive:** write cd/run directive files for the shell shim (T2) ([f1d979e](https://github.com/bfirestone/jj-workspace/commit/f1d979e51010cdcc3ec3487f7cb378605310f4df))
* **fuzzy:** rank workspace names with SkimMatcherV2 (T3) ([df2ae86](https://github.com/bfirestone/jj-workspace/commit/df2ae86e12e452644ecc6edcae66173ad86d0846))
* **jj:** implement workspace listing, preview, and actions (T1) ([823b462](https://github.com/bfirestone/jj-workspace/commit/823b46296f7bdf93337737b937998626da88e255))
* **main:** wire picker loop, /dev/tty render, directives, shell init (T8) ([42d00df](https://github.com/bfirestone/jj-workspace/commit/42d00df96041d1028ce260c2468a677a9c19a80f))
* scaffold jw crate with shared contracts (T0) ([bb4f8ce](https://github.com/bfirestone/jj-workspace/commit/bb4f8ceb6c4745df2d28b2ddf5583a9bd1a481d3))
* **shell:** emit zsh/bash/fish shims for cd-on-exit (T4) ([1179ae7](https://github.com/bfirestone/jj-workspace/commit/1179ae7f6b047fb2e70ef68c6ff2093e8f8f02e4))
* **ui:** ratatui list + preview + footer rendering (T7) ([d1d9750](https://github.com/bfirestone/jj-workspace/commit/d1d9750e26c3b885b4affb4dbfc57312b4956906))


### Bug Fixes

* **app:** validate empty new-name + harden forget guard (T6 review) ([4ded2a3](https://github.com/bfirestone/jj-workspace/commit/4ded2a358e1f61b32dc6d7e903320114cffef405))
* **main:** friendlier error when not run in a terminal ([4ae8b1e](https://github.com/bfirestone/jj-workspace/commit/4ae8b1e4e6320106df484b19fd0017d98c81b1df))
* **main:** RAII terminal guard + graceful forget refresh (T8 review) ([01afbfd](https://github.com/bfirestone/jj-workspace/commit/01afbfd7b2147291b4fc78c233bfc4864ab88f0d))
