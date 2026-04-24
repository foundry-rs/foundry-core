# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.20.0](https://github.com/foundry-rs/foundry-core/releases/tag/0.20.0) - 2026-04-24

### Bug Fixes

- [compilers] Restore removed `openssl` feature ([#46](https://github.com/foundry-rs/foundry-core/issues/46))
- Restore changelogs from main, confirm before actions in release script

### Dependencies

- [deps] Update sha2 requirement from 0.10 to 0.11 ([#33](https://github.com/foundry-rs/foundry-core/issues/33))
- Inline compilers inter-crate deps ([#29](https://github.com/foundry-rs/foundry-core/issues/29))
- Inline compilers inter-crate deps, keep only cross-group dep in workspace

### Features

- Add local interactive release flow ([#25](https://github.com/foundry-rs/foundry-core/issues/25))

### Miscellaneous Tasks

- Remove `cfg-if` dependency ([#40](https://github.com/foundry-rs/foundry-core/issues/40))
- Replace CI OIDC release with local release flow

### Styling

- Remove `similar-asserts` dependency ([#39](https://github.com/foundry-rs/foundry-core/issues/39))

## [0.19.14](https://github.com/foundry-rs/foundry-core/releases/tag/compilers-v0.19.14) - 2026-04-22

### Bug Fixes

- [compilers] Allow missing `missing_const_for_fn` to prevent error in windows
- [compiler] Pin reqwest to workspace to prevent deny error
- [compilers] Make clippy happy
- [compilers] Fix typos
- [compilers] Remove useless files after migration
- [report] Handle BrokenPipe in BasicStdoutReporter ([#367](https://github.com/foundry-rs/foundry-core/issues/367))
- Read runtime sourcemap strings from deployed bytecode ([#366](https://github.com/foundry-rs/foundry-core/issues/366))
- [ci] Allow OpenSSL license in cargo-deny and fix clippy iter_kv_map lint ([#359](https://github.com/foundry-rs/foundry-core/issues/359))
- Use absolute path for exists() check in resolve_library_import ([#355](https://github.com/foundry-rs/foundry-core/issues/355))
- Disable sparse output optimization when AST is requested ([#352](https://github.com/foundry-rs/foundry-core/issues/352))
- Apply remappings to resolved relative import paths ([#353](https://github.com/foundry-rs/foundry-core/issues/353))
- Match artifact by profile when writing extra output files ([#350](https://github.com/foundry-rs/foundry-core/issues/350))
- Handle solc versions with +commit ([#341](https://github.com/foundry-rs/foundry-core/issues/341))
- Preserve pre release version ([#340](https://github.com/foundry-rs/foundry-core/issues/340))
- Preserve version to install if prerelease ([#339](https://github.com/foundry-rs/foundry-core/issues/339))
- Always mark mocks as dirty ([#335](https://github.com/foundry-rs/foundry-core/issues/335))
- Finalize_imports node ordering ([#329](https://github.com/foundry-rs/foundry-core/issues/329))
- Resolve imports at the end ([#326](https://github.com/foundry-rs/foundry-core/issues/326))
- Make sources' paths absolute ([#312](https://github.com/foundry-rs/foundry-core/issues/312))
- Remove superfluous assertion ([#304](https://github.com/foundry-rs/foundry-core/issues/304))
- [flatten] Sort by loc path and loc start ([#302](https://github.com/foundry-rs/foundry-core/issues/302))
- Allow single sol file remappings ([#295](https://github.com/foundry-rs/foundry-core/issues/295))
- Consistent handle of unresolved imports ([#294](https://github.com/foundry-rs/foundry-core/issues/294))
- Fix typos in comments and variable names across solc-related modules ([#286](https://github.com/foundry-rs/foundry-core/issues/286))
- Is_dirty to use additional_files ([#268](https://github.com/foundry-rs/foundry-core/issues/268))
- Allow top level event declarations ([#251](https://github.com/foundry-rs/foundry-core/issues/251))
- Ordering for flattener ([#247](https://github.com/foundry-rs/foundry-core/issues/247))
- Correctly merge restrictions ([#234](https://github.com/foundry-rs/foundry-core/issues/234))
- Add fallback parser for contract names ([#229](https://github.com/foundry-rs/foundry-core/issues/229))
- Fix minor grammatical issue in project documentation ([#226](https://github.com/foundry-rs/foundry-core/issues/226))
- Imports regex fallback ([#225](https://github.com/foundry-rs/foundry-core/issues/225))
- Re-add version regex parsing ([#223](https://github.com/foundry-rs/foundry-core/issues/223))
- `collect_contract_names` ([#221](https://github.com/foundry-rs/foundry-core/issues/221))
- [tests] Always try installing pinned solc ([#217](https://github.com/foundry-rs/foundry-core/issues/217))
- Outdated merge build error
- Correctly handle b as pre-release in Vyper version ([#213](https://github.com/foundry-rs/foundry-core/issues/213))
- Accept partial first sourcemap element ([#209](https://github.com/foundry-rs/foundry-core/issues/209))
- Include `evm.legacyAssembly` output ([#206](https://github.com/foundry-rs/foundry-core/issues/206))
- Actualize output selection options ([#196](https://github.com/foundry-rs/foundry-core/issues/196))
- [flatten] Update license handling logic ([#184](https://github.com/foundry-rs/foundry-core/issues/184))
- Unify logic for ignored warnings ([#179](https://github.com/foundry-rs/foundry-core/issues/179))
- Remove outdated build infos ([#177](https://github.com/foundry-rs/foundry-core/issues/177))
- Better compatibility with older AST ([#175](https://github.com/foundry-rs/foundry-core/issues/175))
- Fix inconsistent trailing slash in remappings ([#49](https://github.com/foundry-rs/foundry-core/issues/49))
- Doctests ([#154](https://github.com/foundry-rs/foundry-core/issues/154))
- [flatten] Small bugs ([#153](https://github.com/foundry-rs/foundry-core/issues/153))

### Dependencies

- Harden Makefile & CI, remove unused deps found w/ cargo shear ([#18](https://github.com/foundry-rs/foundry-core/issues/18))
- Update to rust edition 2024, bump MSRV to 1.93 ([#364](https://github.com/foundry-rs/foundry-core/issues/364))
- [deps] Bump solar ([#315](https://github.com/foundry-rs/foundry-core/issues/315))
- [deps] Switch to solar meta crate ([#307](https://github.com/foundry-rs/foundry-core/issues/307))
- Update deps + fix clippy ([#297](https://github.com/foundry-rs/foundry-core/issues/297))
- Update deps ([#290](https://github.com/foundry-rs/foundry-core/issues/290))
- Bump solar + MSRV ([#289](https://github.com/foundry-rs/foundry-core/issues/289))
- Bump vyper to 0.4.3 which adds support for `prague` ([#285](https://github.com/foundry-rs/foundry-core/issues/285))
- [deps] Bump dirs ([#243](https://github.com/foundry-rs/foundry-core/issues/243))
- [deps] Bump solar 0.1.1 ([#237](https://github.com/foundry-rs/foundry-core/issues/237))
- Bump MSRV to 1.83 ([#230](https://github.com/foundry-rs/foundry-core/issues/230))

### Documentation

- Link to previous repositories ([#12](https://github.com/foundry-rs/foundry-core/issues/12))
- Add README for each crate ([#11](https://github.com/foundry-rs/foundry-core/issues/11))
- Link to previous repositories
- Remove usage sections, add missing feature flags
- Add README for each crate
- Fix typos ([#202](https://github.com/foundry-rs/foundry-core/issues/202))

### Features

- Migrate `foundry-compilers` ([#9](https://github.com/foundry-rs/foundry-core/issues/9))
- [compilers] Integrate to workpace
- Add ignored_error_codes_from option to Project and ProjectBuilder ([#361](https://github.com/foundry-rs/foundry-core/issues/361))
- Detect Solar compiler from version metadata ([#357](https://github.com/foundry-rs/foundry-core/issues/357))
- Add `SourceParser` ([#300](https://github.com/foundry-rs/foundry-core/issues/300))
- Add support for preprocessing sources ([#252](https://github.com/foundry-rs/foundry-core/issues/252))
- Impl `.path(&self)` for `ContractInfo` ([#250](https://github.com/foundry-rs/foundry-core/issues/250))
- Better artifact filenames for different profiles ([#241](https://github.com/foundry-rs/foundry-core/issues/241))
- Add more features to reduce dependencies ([#239](https://github.com/foundry-rs/foundry-core/issues/239))
- Allow multiple compiler configs ([#170](https://github.com/foundry-rs/foundry-core/issues/170))
- Replace solang with solar ([#215](https://github.com/foundry-rs/foundry-core/issues/215))
- Better extra_args handling ([#208](https://github.com/foundry-rs/foundry-core/issues/208))
- Better error messages for incompatible versions ([#200](https://github.com/foundry-rs/foundry-core/issues/200))
- Always provide `Default` for `MultiCompiler` ([#188](https://github.com/foundry-rs/foundry-core/issues/188))
- [vyper] Add experimental codegen to settings ([#186](https://github.com/foundry-rs/foundry-core/issues/186))
- More user-friendly error when no compiler is available ([#185](https://github.com/foundry-rs/foundry-core/issues/185))
- Add `eofVersion` config option ([#174](https://github.com/foundry-rs/foundry-core/issues/174))
- Allow passing extra cli args to solc + some cleanup ([#171](https://github.com/foundry-rs/foundry-core/issues/171))
- Respect `paths.libraries` for Vyper ([#159](https://github.com/foundry-rs/foundry-core/issues/159))

### Miscellaneous Tasks

- Per-crate cliff.toml and release.toml for independent releases
- Make clippy happy ([#334](https://github.com/foundry-rs/foundry-core/issues/334))
- Remove feature(doc_auto_cfg) ([#327](https://github.com/foundry-rs/foundry-core/issues/327))
- Use FnMut instead of FnOnce + Copy ([#310](https://github.com/foundry-rs/foundry-core/issues/310))
- Use svm instead of manual svm dir logic ([#301](https://github.com/foundry-rs/foundry-core/issues/301))
- Add more instrumentation ([#293](https://github.com/foundry-rs/foundry-core/issues/293))
- Add language matcher on `MultiCompilerLanguage` ([#276](https://github.com/foundry-rs/foundry-core/issues/276))
- Switch to `Prague` hardfork by default ([#272](https://github.com/foundry-rs/foundry-core/issues/272))
- Clean up error! calls ([#273](https://github.com/foundry-rs/foundry-core/issues/273))
- Simplify pragma parsing ([#260](https://github.com/foundry-rs/foundry-core/issues/260))
- Fix spelling issues ([#248](https://github.com/foundry-rs/foundry-core/issues/248))
- Clippy + winnow 0.7 ([#244](https://github.com/foundry-rs/foundry-core/issues/244))
- Call shrink_to_fit afte parsing source maps ([#242](https://github.com/foundry-rs/foundry-core/issues/242))
- More lints ([#238](https://github.com/foundry-rs/foundry-core/issues/238))
- Don't color punctuation in output diagnostics ([#222](https://github.com/foundry-rs/foundry-core/issues/222))
- Remove outdated `ref` patterns ([#218](https://github.com/foundry-rs/foundry-core/issues/218))
- Use Version::new over .parse ([#220](https://github.com/foundry-rs/foundry-core/issues/220))
- Allow adding vyper sources with `add_raw_source` w/ `.vy` / `.vyi` extension ([#211](https://github.com/foundry-rs/foundry-core/issues/211))
- Proper generate legacy asm extra output file ([#207](https://github.com/foundry-rs/foundry-core/issues/207))
- Clippy ([#204](https://github.com/foundry-rs/foundry-core/issues/204))
- Clippy happy ([#195](https://github.com/foundry-rs/foundry-core/issues/195))
- Improve stripping file prefixes ([#164](https://github.com/foundry-rs/foundry-core/issues/164))
- Improve some trace-level logs ([#163](https://github.com/foundry-rs/foundry-core/issues/163))
- Remove most impl AsRef<str,Path> ([#157](https://github.com/foundry-rs/foundry-core/issues/157))
- Clarify version cache lock ([#160](https://github.com/foundry-rs/foundry-core/issues/160))
- Sort derives, derive Eq more ([#161](https://github.com/foundry-rs/foundry-core/issues/161))
- Rename foundry-compilers-project into foundry-compilers ([#152](https://github.com/foundry-rs/foundry-core/issues/152))

### Other

- Merge branch 'main' into steven/add-channel-db
- Harden makefile, run shear
- Add 'crates/compilers/' from commit '017868ce2bf58240b8b7197b735ae5ab39c26d72'
- Remove duplicate assembly check in is_dirty ([#292](https://github.com/foundry-rs/foundry-core/issues/292))
- Remove EOF version field ([#279](https://github.com/foundry-rs/foundry-core/issues/279))
- Move deny to ci ([#233](https://github.com/foundry-rs/foundry-core/issues/233))
- Symlink readme

### Performance

- Switch md5 to xxhash ([#262](https://github.com/foundry-rs/foundry-core/issues/262))
- Don't request unnecessary output ([#231](https://github.com/foundry-rs/foundry-core/issues/231))

### Refactor

- Cache/is_dirty impls ([#311](https://github.com/foundry-rs/foundry-core/issues/311))
- Make Contract generic for Compiler and add metadata to CompilerOutput ([#224](https://github.com/foundry-rs/foundry-core/issues/224))
- Unify sources and filtered sources ([#162](https://github.com/foundry-rs/foundry-core/issues/162))

<!-- generated by git-cliff -->
