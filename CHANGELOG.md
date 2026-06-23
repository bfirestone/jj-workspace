# Changelog

## [0.5.0](https://github.com/bfirestone/jj-workspace/compare/jw-v0.4.0...jw-v0.5.0) (2026-06-23)


### Features

* **cli:** add switch --print-path + lock in remove exit-code regression tests ([3005aa5](https://github.com/bfirestone/jj-workspace/commit/3005aa50d13d148381106a0084b137e71e465a26))

## [0.4.0](https://github.com/bfirestone/jj-workspace/compare/jw-v0.3.0...jw-v0.4.0) (2026-06-23)


### Features

* **cli:** jw self update command + README docs ([8653b6d](https://github.com/bfirestone/jj-workspace/commit/8653b6dd495ca0c9e93364c39cb4ddf30db8c337))
* **selfupdate:** foundation deps + pure helpers for self-update ([d35ad2e](https://github.com/bfirestone/jj-workspace/commit/d35ad2e31077b225e83fc7b100d0da582928405b))
* **selfupdate:** GitHub resolve/download/verify/extract + run_update ([039550f](https://github.com/bfirestone/jj-workspace/commit/039550f53005ae3f76ef29085d4c3c072053320f))


### Bug Fixes

* **selfupdate:** bail on non-zero codesign exit status in resign_macos ([0de702a](https://github.com/bfirestone/jj-workspace/commit/0de702af39483563f5ac2ff4823ccaf42672cb5b))

## [0.3.0](https://github.com/bfirestone/jj-workspace/compare/jw-v0.2.0...jw-v0.3.0) (2026-06-23)


### Features

* add curl-based installer script ([77445c1](https://github.com/bfirestone/jj-workspace/commit/77445c195f5500a3bc1132eb996b7d36ec37084b))
* **cli:** jw switch / remove + seed-aware picker create + delete-on-forget ([042db0c](https://github.com/bfirestone/jj-workspace/commit/042db0c3b71827b7ea8792552e625db93112f104))
* **jj,ops:** foundation types for workspace CRUD + no-repo classify ([a20a966](https://github.com/bfirestone/jj-workspace/commit/a20a9662191acb18c2128d137bb9812ba4a4e7a2))
* **keys:** configurable keybindings via [keys] config table ([e388873](https://github.com/bfirestone/jj-workspace/commit/e3888737df69ce263c56c24f9735c81dc275641b))
* **main:** clean jj-style hint when run outside a jj repo ([ad15bf9](https://github.com/bfirestone/jj-workspace/commit/ad15bf93611fefc99c62eaffffbe29ad6617ad70))
* **ops:** switch (create-or-go) + guarded remove orchestration ([d21f029](https://github.com/bfirestone/jj-workspace/commit/d21f029cd0af07df3b4d2bd5cc47a7bb53d73e96))


### Bug Fixes

* **cli:** vary run_remove prompt and success message on --keep ([dd90f1d](https://github.com/bfirestone/jj-workspace/commit/dd90f1d7520b4d9516018af9ac136e6591207512))
* **ops:** delete dir before forget, drop module prefix, pin switch/remove sigs ([8f242b1](https://github.com/bfirestone/jj-workspace/commit/8f242b125303341cad81f2733701b84506870d31))


### Performance

* **jj:** resolve workspace roots from the list template, not N shell-outs ([a00ba92](https://github.com/bfirestone/jj-workspace/commit/a00ba927164b3db617e06c06eda867f6fd2a94aa))

## [0.2.0](https://github.com/bfirestone/jj-workspace/compare/jw-v0.1.0...jw-v0.2.0) (2026-06-23)


### Features

* **shell:** add `config shell install` to write the shim into rc files ([bf268be](https://github.com/bfirestone/jj-workspace/commit/bf268be56d299f89d47197f0fec9c14a9b26cf4f))
* **ui:** config-driven color themes ([d7ffe18](https://github.com/bfirestone/jj-workspace/commit/d7ffe18e851b045ae3f0f4842baf476ddeb167e9))


### Performance

* **jj:** parallelize workspace root resolution for sub-100ms first paint ([1d7fc12](https://github.com/bfirestone/jj-workspace/commit/1d7fc12baa77dc61eba49b1f53464173286d73af))

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
