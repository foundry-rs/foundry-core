# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0000-00-00

### foundry-core

#### Features

- Migrate `foundry-block-explorers`/`foundry-blob-explorers` ([#14](https://github.com/foundry-rs/foundry-core/pull/14))
- Migrate `foundry-compilers` ([#9](https://github.com/foundry-rs/foundry-core/pull/9))
- Migrate `foundry-fork-db` ([#7](https://github.com/foundry-rs/foundry-core/pull/7))

#### Documentation

- Link to previous repositories ([#12](https://github.com/foundry-rs/foundry-core/pull/12))
- Add README for each crate ([#11](https://github.com/foundry-rs/foundry-core/pull/11))

#### Bug Fixes

- Fix test server address to support all platforms ([#10](https://github.com/foundry-rs/foundry-core/pull/10))

#### Miscellaneous Tasks

- Reorder workspace deps and normalize formatting ([#13](https://github.com/foundry-rs/foundry-core/pull/13))
- Enhance CI, add CodeQL ([#4](https://github.com/foundry-rs/foundry-core/pull/4))
- Basic repo layout ([#1](https://github.com/foundry-rs/foundry-core/pull/1))

## 2026-04-16

### fork-db v0.25.0

#### Dependencies

- Bump revm ([#136](https://github.com/foundry-rs/foundry-fork-db/issues/136))
- Bump alloy 2.0.0 ([#132](https://github.com/foundry-rs/foundry-fork-db/issues/132))
- [deps] Add 7 day dependency cooldown ([#131](https://github.com/foundry-rs/foundry-fork-db/issues/131))
- Update to Rust edition 2024, sync lint rules with foundry ([#129](https://github.com/foundry-rs/foundry-fork-db/issues/129))

#### Features

- `ForkBlockEnv` marker trait ([#128](https://github.com/foundry-rs/foundry-fork-db/issues/128))
- `BackendHandler`/`SharedBackend` generic block ([#125](https://github.com/foundry-rs/foundry-fork-db/issues/125))
- `JsonBlockCacheDB` generic block + Serialization workaround ([#123](https://github.com/foundry-rs/foundry-fork-db/issues/123))
- `BlockchainDbMeta` generic over `BlockEnv` ([#122](https://github.com/foundry-rs/foundry-fork-db/issues/122))

#### Miscellaneous Tasks

- Set `BlockchainDb` default `BlockEnv` ([#126](https://github.com/foundry-rs/foundry-fork-db/issues/126))
- `missing_const_for_fn` lint back to "warn" ([#124](https://github.com/foundry-rs/foundry-fork-db/issues/124))
- Update CODEOWNERS ([#117](https://github.com/foundry-rs/foundry-fork-db/issues/117))

#### Other

- Pin GitHub Actions to SHA, add cargo to dependabot ([#133](https://github.com/foundry-rs/foundry-fork-db/issues/133))

#### Refactor

- Remove tempo-revm dep, use Serialize+DeserializeOwned for ForkBlockEnv ([#135](https://github.com/foundry-rs/foundry-fork-db/issues/135))

## 2026-03-13

### fork-db v0.24.1

#### Bug Fixes

- Use revm's maps ([#113](https://github.com/foundry-rs/foundry-fork-db/issues/113))

### fork-db v0.24.0

#### Dependencies

- Bump revm to 36.0.0 ([#112](https://github.com/foundry-rs/foundry-fork-db/issues/112))
- Bump MSRV to 1.91 ([#108](https://github.com/foundry-rs/foundry-fork-db/issues/108))

## 2026-02-11

### fork-db v0.23.0

#### Features

- [backend] Generic `Network` ([#103](https://github.com/foundry-rs/foundry-fork-db/issues/103))

#### Other

- Update to tempoxyz ([#99](https://github.com/foundry-rs/foundry-fork-db/issues/99))

## 2026-01-22

### compilers v0.19.14

#### Bug Fixes

- Use absolute path for exists() check in resolve_library_import ([#355](https://github.com/foundry-rs/compilers/issues/355))
- Disable sparse output optimization when AST is requested ([#352](https://github.com/foundry-rs/compilers/issues/352))
- Apply remappings to resolved relative import paths ([#353](https://github.com/foundry-rs/compilers/issues/353))
- Match artifact by profile when writing extra output files ([#350](https://github.com/foundry-rs/compilers/issues/350))
- Add snake_case aliases for ModelCheckerSettings fields ([#348](https://github.com/foundry-rs/compilers/issues/348))
- Normalize Vyper EVM version during input creation ([#345](https://github.com/foundry-rs/compilers/issues/345))
- Sort remapping candidates to avoid non deterministic output ([#343](https://github.com/foundry-rs/compilers/issues/343))

#### Dependencies

- Bump version, prepare for release
- Bump 0.19.12 ([#347](https://github.com/foundry-rs/compilers/issues/347))

#### Other

- Update to tempoxyz ([#344](https://github.com/foundry-rs/compilers/issues/344))

## 2026-01-16

### fork-db v0.22.0

#### Dependencies

- [deps] Bump revm from 33.0.0 to 34.0.0 ([#98](https://github.com/foundry-rs/foundry-fork-db/issues/98))

## 2025-11-19

### compilers v0.19.10

#### Bug Fixes

- Handle solc versions with +commit ([#341](https://github.com/foundry-rs/compilers/issues/341))

### compilers v0.19.9

#### Bug Fixes

- Preserve pre release version ([#340](https://github.com/foundry-rs/compilers/issues/340))

## 2025-11-18

### compilers v0.19.7

#### Bug Fixes

- Preserve version to install if prerelease ([#339](https://github.com/foundry-rs/compilers/issues/339))
- Always mark mocks as dirty ([#335](https://github.com/foundry-rs/compilers/issues/335))
- Expose VyperSourceLocation fields ([#333](https://github.com/foundry-rs/compilers/issues/333))
- Finalize_imports node ordering ([#329](https://github.com/foundry-rs/compilers/issues/329))
- Resolve imports at the end ([#326](https://github.com/foundry-rs/compilers/issues/326))
- Make sources' paths absolute ([#312](https://github.com/foundry-rs/compilers/issues/312))
- Sanitize `stopAfter` ([#309](https://github.com/foundry-rs/compilers/issues/309))
- Remove superfluous assertion ([#304](https://github.com/foundry-rs/compilers/issues/304))
- [flatten] Sort by loc path and loc start ([#302](https://github.com/foundry-rs/compilers/issues/302))

#### Dependencies

- Bump 0.19.6 ([#337](https://github.com/foundry-rs/compilers/issues/337))
- [`ci`] Pin deps in workflow and add `dependabot` to update them weekly ([#321](https://github.com/foundry-rs/compilers/issues/321))
- [deps] Bump solar ([#315](https://github.com/foundry-rs/compilers/issues/315))
- [deps] Switch to solar meta crate ([#307](https://github.com/foundry-rs/compilers/issues/307))
- [deps] Bump to 0.18.3 ([#303](https://github.com/foundry-rs/compilers/issues/303))
- Update deps + fix clippy ([#297](https://github.com/foundry-rs/compilers/issues/297))

#### Features

- Add `SourceParser` ([#300](https://github.com/foundry-rs/compilers/issues/300))

#### Miscellaneous Tasks

- Make clippy happy ([#334](https://github.com/foundry-rs/compilers/issues/334))
- [`ci`] Merge in `codeql.yml` and mark as requirement for ci-success ([#331](https://github.com/foundry-rs/compilers/issues/331))
- Remove feature(doc_auto_cfg) ([#327](https://github.com/foundry-rs/compilers/issues/327))
- [`ci`] Rescope permissions according to principle of least privilege ([#323](https://github.com/foundry-rs/compilers/issues/323))
- [`ci`] Harden workflow by setting default permission to read only ([#320](https://github.com/foundry-rs/compilers/issues/320))
- [`ci`] Add CodeQL ([#319](https://github.com/foundry-rs/compilers/issues/319))
- [`ci`] Add `ci-success` step so we can make this a condition for merging ([#316](https://github.com/foundry-rs/compilers/issues/316))
- Use FnMut instead of FnOnce + Copy ([#310](https://github.com/foundry-rs/compilers/issues/310))
- Use svm instead of manual svm dir logic ([#301](https://github.com/foundry-rs/compilers/issues/301))
- Add @0xrusowsky to `CODEOWNERS` ([#299](https://github.com/foundry-rs/compilers/issues/299))
- Update `CODEOWNERS` to improve visibility ([#298](https://github.com/foundry-rs/compilers/issues/298))

#### Performance

- Improve linking implementation ([#324](https://github.com/foundry-rs/compilers/issues/324))
- Parallelize Remapping::get_many ([#314](https://github.com/foundry-rs/compilers/issues/314))

#### Refactor

- Cache/is_dirty impls ([#311](https://github.com/foundry-rs/compilers/issues/311))

## 2025-11-12

### fork-db v0.21.0

#### Dependencies

- Bump to revm 33 ([#87](https://github.com/foundry-rs/foundry-fork-db/issues/87))

## 2025-11-05

### fork-db v0.20.0

#### Dependencies

- Bump revm 31 ([#85](https://github.com/foundry-rs/foundry-fork-db/issues/85))

#### Features

- Erase provider generic ([#80](https://github.com/foundry-rs/foundry-fork-db/issues/80))

#### Miscellaneous Tasks

- Use inspect_err ([#81](https://github.com/foundry-rs/foundry-fork-db/issues/81))

## 2025-10-15

### fork-db v0.19.0

#### Dependencies

- [deps] Bump `revm` to `30.1.1` ([#77](https://github.com/foundry-rs/foundry-fork-db/issues/77))
- [`ci`] Pin deps in workflow and add `dependabot` to update them weekly ([#66](https://github.com/foundry-rs/foundry-fork-db/issues/66))

#### Miscellaneous Tasks

- Rm `doc_auto_cfg` ([#75](https://github.com/foundry-rs/foundry-fork-db/issues/75))
- [`ci`] Clean up workflow + harden workflow by setting default permission to read only ([#65](https://github.com/foundry-rs/foundry-fork-db/issues/65))

#### Other

- Merge in codeql.yml as mark as requirement for ci-success ([#70](https://github.com/foundry-rs/foundry-fork-db/issues/70))
- Rescope permissions ([#69](https://github.com/foundry-rs/foundry-fork-db/issues/69))
- Add codeql ([#64](https://github.com/foundry-rs/foundry-fork-db/issues/64))
- Add ci-success ([#63](https://github.com/foundry-rs/foundry-fork-db/issues/63))

## 2025-09-01

### explorers v0.22.0

#### Bug Fixes

- [serde] Support 0x-prefixed hex in deserialize_stringified_block_number ([#100](https://github.com/foundry-rs/block-explorers/issues/100))
- [tests] Disambiguate `Into` target for `parse_units` in `gas.rs` ([#98](https://github.com/foundry-rs/block-explorers/issues/98))
- `dependencies.yml` is unused ([#104](https://github.com/foundry-rs/block-explorers/issues/104))

#### Dependencies

- [deps] Bump compilers 0.19.0 ([#107](https://github.com/foundry-rs/block-explorers/issues/107))
- [deps] Add dependencies ci workflow + update deps + fix clippy ([#102](https://github.com/foundry-rs/block-explorers/issues/102))

#### Miscellaneous Tasks

- Deprecate Etherscan V1 ([#101](https://github.com/foundry-rs/block-explorers/issues/101))
- Add @0xrusowsky to `CODEOWNERS` ([#105](https://github.com/foundry-rs/block-explorers/issues/105))
- Update `CODEOWNERS` to improve visibility ([#103](https://github.com/foundry-rs/block-explorers/issues/103))

## 2025-08-29

### fork-db v0.18.1

#### Other

- Bumps to alloy-hardforks 0.3.0 ([#62](https://github.com/foundry-rs/foundry-fork-db/issues/62))

## 2025-08-25

### fork-db v0.18.0

#### Dependencies

- Update deps ([#58](https://github.com/foundry-rs/foundry-fork-db/issues/58))

#### Other

- Add @0xrusowsky ([#60](https://github.com/foundry-rs/foundry-fork-db/issues/60))
- Update codeowners to improve visibility ([#59](https://github.com/foundry-rs/foundry-fork-db/issues/59))

## 2025-08-18

### fork-db v0.17.0

#### Dependencies

- [deps] Bump revm 28.0.0, msrv 1.88 required for revm ([#57](https://github.com/foundry-rs/foundry-fork-db/issues/57))

## 2025-08-01

### compilers v0.18.2

#### Bug Fixes

- Allow single sol file remappings ([#295](https://github.com/foundry-rs/compilers/issues/295))

## 2025-07-31

### compilers v0.18.1

#### Bug Fixes

- Consistent handle of unresolved imports ([#294](https://github.com/foundry-rs/compilers/issues/294))

#### Miscellaneous Tasks

- Add more instrumentation ([#293](https://github.com/foundry-rs/compilers/issues/293))

#### Other

- Remove duplicate assembly check in is_dirty ([#292](https://github.com/foundry-rs/compilers/issues/292))

## 2025-07-14

### compilers v0.18.0

#### Dependencies

- Bump to 0.18.0
- Update deps ([#290](https://github.com/foundry-rs/compilers/issues/290))
- Bump solar + MSRV ([#289](https://github.com/foundry-rs/compilers/issues/289))

### explorers v0.20.0

#### Dependencies

- Bump solar + MSRV ([#96](https://github.com/foundry-rs/block-explorers/issues/96))

#### Miscellaneous Tasks

- Add trace for getabi ([#92](https://github.com/foundry-rs/block-explorers/issues/92))
- Update CI flow, add workflow_dispatch, remove unused GOERLI_PRIVATE_KEY ([#95](https://github.com/foundry-rs/block-explorers/issues/95))

## 2025-07-10

### fork-db v0.16.0

#### Dependencies

- [deps] Bump revm 27.0.2 ([#53](https://github.com/foundry-rs/foundry-fork-db/issues/53))

#### Features

- Support  getAccount mode ([#48](https://github.com/foundry-rs/foundry-fork-db/issues/48))

#### Miscellaneous Tasks

- Add trace for successful cache load ([#55](https://github.com/foundry-rs/foundry-fork-db/issues/55))
- Make clippy happy ([#54](https://github.com/foundry-rs/foundry-fork-db/issues/54))

## 2025-07-03

### explorers v0.19.1

#### Bug Fixes

- Etherscan V2 API URLs from `alloy-chains` already contain `chainid` ([#93](https://github.com/foundry-rs/block-explorers/issues/93))

## 2025-06-30

### compilers v0.17.4

#### Bug Fixes

- Fix typos in comments and variable names across solc-related modules ([#286](https://github.com/foundry-rs/compilers/issues/286))

#### Dependencies

- Bump vyper to 0.4.3 which adds support for `prague` ([#285](https://github.com/foundry-rs/compilers/issues/285))

#### Miscellaneous Tasks

- Upstreamed  `strip_bytecode_placeholders` from foundry ([#287](https://github.com/foundry-rs/compilers/issues/287))

### explorers v0.19.0

#### Miscellaneous Tasks

- Rustmft

#### Other

- Support vyper-json codeformat ([#91](https://github.com/foundry-rs/block-explorers/issues/91))

## 2025-06-14

### compilers v0.17.3

#### Other

- Revert "fix: implement proper serde handling for unknown AST node typ… ([#284](https://github.com/foundry-rs/compilers/issues/284))

## 2025-06-13

### fork-db v0.15.1

#### Dependencies

- Bump MSRV from 1.83 to 1.85 ([#52](https://github.com/foundry-rs/foundry-fork-db/issues/52))

#### Performance

- Clone for serializing ([#51](https://github.com/foundry-rs/foundry-fork-db/issues/51))

## 2025-06-10

### compilers v0.17.2

#### Bug Fixes

- Implement proper serde handling for unknown AST node types ([#280](https://github.com/foundry-rs/compilers/issues/280))

#### Other

- Add missing node types ([#282](https://github.com/foundry-rs/compilers/issues/282))
- Remove EOF version field ([#279](https://github.com/foundry-rs/compilers/issues/279))

## 2025-06-02

### compilers v0.17.1

#### Dependencies

- Update MSRV policy, bump to `1.87` in `clippy.toml` in line with `CI` and `Cargo.toml` ([#277](https://github.com/foundry-rs/compilers/issues/277))

#### Miscellaneous Tasks

- Add language matcher on `MultiCompilerLanguage` ([#276](https://github.com/foundry-rs/compilers/issues/276))

## 2025-05-29

### compilers v0.16.4

#### Dependencies

- Bump solar v0.1.4 ([#275](https://github.com/foundry-rs/compilers/issues/275))

### explorers v0.18.0

#### Dependencies

- Bump compilers v0.17.0 ([#90](https://github.com/foundry-rs/block-explorers/issues/90))

## 2025-05-28

### compilers v0.16.3

#### Bug Fixes

- Update Tera documentation link in cliff.toml ([#270](https://github.com/foundry-rs/compilers/issues/270))

#### Miscellaneous Tasks

- Switch to `Prague` hardfork by default ([#272](https://github.com/foundry-rs/compilers/issues/272))
- Clean up error! calls ([#273](https://github.com/foundry-rs/compilers/issues/273))

#### Other

- Some fields are optional during `"stopAfter":"parsing"` ([#271](https://github.com/foundry-rs/compilers/issues/271))

## 2025-05-23

### fork-db v0.15.0

#### Dependencies

- Bump revm to 24.0.0 ([#50](https://github.com/foundry-rs/foundry-fork-db/issues/50))

## 2025-05-21

### compilers v0.16.2

#### Other

- Support `transient` in `StorageLocation` ([#269](https://github.com/foundry-rs/compilers/issues/269))

## 2025-05-16

### compilers v0.16.1

#### Bug Fixes

- Is_dirty to use additional_files ([#268](https://github.com/foundry-rs/compilers/issues/268))

### explorers v0.17.0

#### Dependencies

- Alloy 1.0 ([#89](https://github.com/foundry-rs/block-explorers/issues/89))

## 2025-05-15

### fork-db v0.14.0

#### Miscellaneous Tasks

- Alloy 1.0 ([#49](https://github.com/foundry-rs/foundry-fork-db/issues/49))

## 2025-05-12

### compilers v0.16.0

#### Dependencies

- Bump solar version ([#264](https://github.com/foundry-rs/compilers/issues/264))

### explorers v0.16.0

#### Dependencies

- Bump compilers to 0.16.0 ([#88](https://github.com/foundry-rs/block-explorers/issues/88))

## 2025-05-08

### fork-db v0.13.0

#### Dependencies

- [deps] Alloy 0.15 ([#46](https://github.com/foundry-rs/foundry-fork-db/issues/46))

#### Features

- Bump revm to `21.0.0` and alloy to `0.13.0` ([#44](https://github.com/foundry-rs/foundry-fork-db/issues/44))

## 2025-05-07

### compilers v0.15.0

#### Dependencies

- [deps] Bump alloy 1.0 ([#263](https://github.com/foundry-rs/compilers/issues/263))

#### Documentation

- Update CHANGELOG.md

### explorers v0.14.0

#### Dependencies

- [deps] Bump alloy-core 1.0 + alloy 0.15 ([#86](https://github.com/foundry-rs/block-explorers/issues/86))

### explorers v0.13.3

#### Miscellaneous Tasks

- FromStr for EtherscanApiVersion ([#85](https://github.com/foundry-rs/block-explorers/issues/85))

## 2025-05-05

### explorers v0.13.2

#### Miscellaneous Tasks

- Allow CDLA-Permissive-2.0 ([#84](https://github.com/foundry-rs/block-explorers/issues/84))

#### Other

- Handle parsing and serialization of etherscan api version ([#83](https://github.com/foundry-rs/block-explorers/issues/83))

## 2025-04-19

### compilers v0.14.1

#### Bug Fixes

- Fix Update CONTRIBUTING.md ([#261](https://github.com/foundry-rs/compilers/issues/261))

#### Performance

- Switch md5 to xxhash ([#262](https://github.com/foundry-rs/compilers/issues/262))

## 2025-04-15

### explorers v0.13.1

#### Other

- Update etherscan lib to handle both GET and POST parameters for chainid ([#82](https://github.com/foundry-rs/block-explorers/issues/82))

## 2025-04-07

### compilers v0.14.0

#### Features

- Add support for preprocessing sources ([#252](https://github.com/foundry-rs/compilers/issues/252))

#### Miscellaneous Tasks

- Simplify pragma parsing ([#260](https://github.com/foundry-rs/compilers/issues/260))

#### Styling

- Update file extension for compatibility ([#258](https://github.com/foundry-rs/compilers/issues/258))

### explorers v0.13.0

#### Dependencies

- Bump compilers to v0.14 ([#81](https://github.com/foundry-rs/block-explorers/issues/81))

## 2025-04-02

### explorers v0.12.0

#### Dependencies

- Bump alloy 0.13 ([#80](https://github.com/foundry-rs/block-explorers/issues/80))

## 2025-03-15

### explorers v0.11.2

#### Other

- Add v2 verify routes ([#73](https://github.com/foundry-rs/block-explorers/issues/73))

### explorers v0.11.1

#### Bug Fixes

- Fix tests ([#77](https://github.com/foundry-rs/block-explorers/issues/77))

#### Dependencies

- Bump alloy 0.12 ([#79](https://github.com/foundry-rs/block-explorers/issues/79))
- Bump alloy 0.11 ([#76](https://github.com/foundry-rs/block-explorers/issues/76))

#### Miscellaneous Tasks

- Allow paste ([#78](https://github.com/foundry-rs/block-explorers/issues/78))

## 2025-03-14

### compilers v0.13.5

#### Bug Fixes

- Missing check for normalization ([#257](https://github.com/foundry-rs/compilers/issues/257))

### compilers v0.13.4

#### Bug Fixes

- Update normalization ([#256](https://github.com/foundry-rs/compilers/issues/256))

#### Features

- Add osaka evm version ([#254](https://github.com/foundry-rs/compilers/issues/254))

#### Other

- Allow unmaintained paste ([#255](https://github.com/foundry-rs/compilers/issues/255))

## 2025-03-07

### fork-db v0.12.0

#### Dependencies

- [deps] Alloy 0.12 ([#43](https://github.com/foundry-rs/foundry-fork-db/issues/43))

## 2025-02-18

### fork-db v0.11.1

#### Features

- Expose cache_path for JsonBlockCacheDB ([#42](https://github.com/foundry-rs/foundry-fork-db/issues/42))

## 2025-02-14

### compilers v0.13.3

#### Bug Fixes

- Allow top level event declarations ([#251](https://github.com/foundry-rs/compilers/issues/251))

#### Features

- Impl `.path(&self)` for `ContractInfo` ([#250](https://github.com/foundry-rs/compilers/issues/250))

## 2025-02-06

### compilers v0.13.2

#### Bug Fixes

- Ordering for flattener ([#247](https://github.com/foundry-rs/compilers/issues/247))

#### Miscellaneous Tasks

- Fix spelling issues ([#248](https://github.com/foundry-rs/compilers/issues/248))

## 2025-02-02

### compilers v0.13.1

#### Bug Fixes

- Handle displaying multiline errors correctly ([#245](https://github.com/foundry-rs/compilers/issues/245))

#### Dependencies

- [deps] Bump dirs ([#243](https://github.com/foundry-rs/compilers/issues/243))

#### Miscellaneous Tasks

- Clippy + winnow 0.7 ([#244](https://github.com/foundry-rs/compilers/issues/244))
- Call shrink_to_fit after parsing source maps ([#242](https://github.com/foundry-rs/compilers/issues/242))

## 2025-01-31

### fork-db v0.11.0

#### Dependencies

- Bump alloy 0.11 ([#41](https://github.com/foundry-rs/foundry-fork-db/issues/41))

## 2025-01-21

### compilers v0.13.0

#### Features

- Better artifact filenames for different profiles ([#241](https://github.com/foundry-rs/compilers/issues/241))
- Add more features to reduce dependencies ([#239](https://github.com/foundry-rs/compilers/issues/239))

#### Miscellaneous Tasks

- More lints ([#238](https://github.com/foundry-rs/compilers/issues/238))

### explorers v0.11.0

#### Dependencies

- Bump compilers ([#74](https://github.com/foundry-rs/block-explorers/issues/74))

#### Miscellaneous Tasks

- Update deny.toml ([#71](https://github.com/foundry-rs/block-explorers/issues/71))

#### Other

- Move deny to ci ([#70](https://github.com/foundry-rs/block-explorers/issues/70))

## 2025-01-05

### compilers v0.12.9

#### Bug Fixes

- EvmVersion `from_str` ([#235](https://github.com/foundry-rs/compilers/issues/235))

#### Dependencies

- [deps] Bump solar 0.1.1 ([#237](https://github.com/foundry-rs/compilers/issues/237))

#### Miscellaneous Tasks

- Clippy ([#236](https://github.com/foundry-rs/compilers/issues/236))

## 2024-12-30

### fork-db v0.10.0

#### Features

- Update revm 19 alloy 09 ([#39](https://github.com/foundry-rs/foundry-fork-db/issues/39))

## 2024-12-13

### compilers v0.12.8

#### Bug Fixes

- Correctly merge restrictions ([#234](https://github.com/foundry-rs/compilers/issues/234))

#### Other

- Move deny to ci ([#233](https://github.com/foundry-rs/compilers/issues/233))

## 2024-12-10

### fork-db v0.9.0

#### Dependencies

- Bump alloy 0.8 ([#38](https://github.com/foundry-rs/foundry-fork-db/issues/38))
- Bump MSRV to 1.81 ([#37](https://github.com/foundry-rs/foundry-fork-db/issues/37))
- Bump breaking deps ([#36](https://github.com/foundry-rs/foundry-fork-db/issues/36))

#### Miscellaneous Tasks

- Update deny.toml ([#35](https://github.com/foundry-rs/foundry-fork-db/issues/35))

#### Other

- Move deny to ci ([#34](https://github.com/foundry-rs/foundry-fork-db/issues/34))

## 2024-12-09

### explorers v0.10.0

#### Dependencies

- Bump alloy 0.7 ([#69](https://github.com/foundry-rs/block-explorers/issues/69))

#### Styling

- [`blob-explorers`] Accommodate new blobscan API changes ([#68](https://github.com/foundry-rs/block-explorers/issues/68))

## 2024-12-05

### compilers v0.12.7

#### Bug Fixes

- Vyper version comparison typo ([#232](https://github.com/foundry-rs/compilers/issues/232))

## 2024-12-04

### compilers v0.12.6

#### Performance

- Don't request unnecessary output ([#231](https://github.com/foundry-rs/compilers/issues/231))

### compilers v0.12.5

#### Refactor

- Make Contract generic for Compiler and add metadata to CompilerOutput ([#224](https://github.com/foundry-rs/compilers/issues/224))

## 2024-12-02

### compilers v0.12.4

#### Bug Fixes

- Add fallback parser for contract names ([#229](https://github.com/foundry-rs/compilers/issues/229))
- Fix minor grammatical issue in project documentation ([#226](https://github.com/foundry-rs/compilers/issues/226))

#### Dependencies

- Bump MSRV to 1.83 ([#230](https://github.com/foundry-rs/compilers/issues/230))

#### Other

- Add note about grammar,spelling prs ([#228](https://github.com/foundry-rs/compilers/issues/228))

## 2024-11-28

### fork-db v0.8.0

#### Dependencies

- Bump alloy ([#33](https://github.com/foundry-rs/foundry-fork-db/issues/33))

## 2024-11-27

### fork-db v0.7.2

#### Documentation

- Fix typo in changelog generator 2
- Fix typo in changelog generator

#### Features

- [backend] Add support for arbitrary provider requests with AnyRequest ([#32](https://github.com/foundry-rs/foundry-fork-db/issues/32))

## 2024-11-20

### compilers v0.12.3

#### Bug Fixes

- Imports regex fallback ([#225](https://github.com/foundry-rs/compilers/issues/225))

### compilers v0.12.2

#### Bug Fixes

- Re-add version regex parsing ([#223](https://github.com/foundry-rs/compilers/issues/223))

#### Miscellaneous Tasks

- Don't color punctuation in output diagnostics ([#222](https://github.com/foundry-rs/compilers/issues/222))

## 2024-11-18

### compilers v0.12.1

#### Bug Fixes

- `collect_contract_names` ([#221](https://github.com/foundry-rs/compilers/issues/221))

### compilers v0.12.0

#### Bug Fixes

- Sanitize `settings.optimizer.details.inliner` ([#216](https://github.com/foundry-rs/compilers/issues/216))
- [tests] Always try installing pinned solc ([#217](https://github.com/foundry-rs/compilers/issues/217))
- Outdated merge build error
- Correctly handle b as pre-release in Vyper version ([#213](https://github.com/foundry-rs/compilers/issues/213))

#### Features

- Allow multiple compiler configs ([#170](https://github.com/foundry-rs/compilers/issues/170))
- Replace solang with solar ([#215](https://github.com/foundry-rs/compilers/issues/215))

#### Miscellaneous Tasks

- Remove outdated `ref` patterns ([#218](https://github.com/foundry-rs/compilers/issues/218))
- Inline constants in Settings::sanitize ([#219](https://github.com/foundry-rs/compilers/issues/219))
- Use Version::new over .parse ([#220](https://github.com/foundry-rs/compilers/issues/220))

### explorers v0.9.0

#### Dependencies

- Bump compilers ([#67](https://github.com/foundry-rs/block-explorers/issues/67))

## 2024-11-09

### fork-db v0.7.1

#### Bug Fixes

- Accept generic header in meta builder ([#30](https://github.com/foundry-rs/foundry-fork-db/issues/30))

## 2024-11-08

### fork-db v0.7.0

#### Dependencies

- [deps] Bump alloy 0.6.2 ([#29](https://github.com/foundry-rs/foundry-fork-db/issues/29))

#### Documentation

- Update docs

## 2024-10-23

### fork-db v0.6.0

#### Dependencies

- Bump revm ([#27](https://github.com/foundry-rs/foundry-fork-db/issues/27))

## 2024-10-18

### fork-db v0.5.0

#### Dependencies

- Bump alloy 0.5 ([#26](https://github.com/foundry-rs/foundry-fork-db/issues/26))

## 2024-10-14

### compilers v0.11.5

#### Bug Fixes

- Accept partial first sourcemap element ([#209](https://github.com/foundry-rs/compilers/issues/209))

#### Miscellaneous Tasks

- Allow adding vyper sources with `add_raw_source` w/ `.vy` / `.vyi` extension ([#211](https://github.com/foundry-rs/compilers/issues/211))
- [`ci`] Fix deny (add `ZLib` exception) ([#212](https://github.com/foundry-rs/compilers/issues/212))

## 2024-10-02

### compilers v0.11.4

#### Features

- Better extra_args handling ([#208](https://github.com/foundry-rs/compilers/issues/208))

## 2024-09-30

### compilers v0.11.3

#### Miscellaneous Tasks

- Proper generate legacy asm extra output file ([#207](https://github.com/foundry-rs/compilers/issues/207))

### compilers v0.11.2

#### Bug Fixes

- Include `evm.legacyAssembly` output ([#206](https://github.com/foundry-rs/compilers/issues/206))

#### Documentation

- Fix typos ([#202](https://github.com/foundry-rs/compilers/issues/202))

#### Miscellaneous Tasks

- Clippy ([#204](https://github.com/foundry-rs/compilers/issues/204))
- Use serde_json::from_str ([#203](https://github.com/foundry-rs/compilers/issues/203))

### explorers v0.8.0

#### Miscellaneous Tasks

- Alloy 0.4 ([#65](https://github.com/foundry-rs/block-explorers/issues/65))

### fork-db v0.4.0

#### Dependencies

- Bump alloy 0.4 ([#24](https://github.com/foundry-rs/foundry-fork-db/issues/24))

## 2024-09-29

### fork-db v0.3.2

#### Features

- BlockchainDbMeta builder ([#22](https://github.com/foundry-rs/foundry-fork-db/issues/22))

#### Miscellaneous Tasks

- Use more alloy_primitives::map

## 2024-09-21

### fork-db v0.3.1

#### Dependencies

- [deps] Disable default features for revm ([#20](https://github.com/foundry-rs/foundry-fork-db/issues/20))

#### Other

- Don't deploy docs

## 2024-09-19

### explorers v0.7.3

#### Bug Fixes

- Solc_config settings ([#63](https://github.com/foundry-rs/block-explorers/issues/63))

## 2024-09-17

### compilers v0.11.1

#### Bug Fixes

- Ast Node Bindings ([#199](https://github.com/foundry-rs/compilers/issues/199))
- Actualize output selection options ([#196](https://github.com/foundry-rs/compilers/issues/196))

#### Features

- Better error messages for incompatible versions ([#200](https://github.com/foundry-rs/compilers/issues/200))

#### Miscellaneous Tasks

- Improve error handling in source map parsing ([#201](https://github.com/foundry-rs/compilers/issues/201))
- Clippy happy ([#195](https://github.com/foundry-rs/compilers/issues/195))
- Fix up the README example ([#194](https://github.com/foundry-rs/compilers/issues/194))

## 2024-09-03

### explorers v0.7.1

#### Dependencies

- [deps] Bump compilers ([#62](https://github.com/foundry-rs/block-explorers/issues/62))

## 2024-09-02

### compilers v0.11.0

#### Dependencies

- [deps] Bump alloy ([#193](https://github.com/foundry-rs/compilers/issues/193))

## 2024-08-29

### fork-db v0.3.0

#### Bug Fixes

- Fix fmt

#### Dependencies

- Merge pull request [#19](https://github.com/foundry-rs/foundry-fork-db/issues/19) from foundry-rs/matt/bump-alloy03
- Bump alloy

#### Other

- Update
- Merge pull request [#18](https://github.com/foundry-rs/foundry-fork-db/issues/18) from nkysg/unbound_channel
- Rm clone
- Replace bounded channel with unbounded channel

## 2024-08-28

### explorers v0.6.0

#### Dependencies

- Updated alloy-core and alloy dependencies ([#61](https://github.com/foundry-rs/block-explorers/issues/61))

## 2024-08-27

### explorers v0.5.2

#### Bug Fixes

- Fix bugs about the default EVM version in Solc ([#59](https://github.com/foundry-rs/block-explorers/issues/59))

#### Miscellaneous Tasks

- Improve invalid key checks ([#58](https://github.com/foundry-rs/block-explorers/issues/58))

#### Testing

- Add invalid api key response test ([#57](https://github.com/foundry-rs/block-explorers/issues/57))

## 2024-08-26

### compilers v0.10.3

#### Bug Fixes

- [flatten] Update license handling logic ([#184](https://github.com/foundry-rs/compilers/issues/184))

#### Documentation

- Docs fix spelling issues ([#190](https://github.com/foundry-rs/compilers/issues/190))

#### Features

- Always provide `Default` for `MultiCompiler` ([#188](https://github.com/foundry-rs/compilers/issues/188))
- [vyper] Add experimental codegen to settings ([#186](https://github.com/foundry-rs/compilers/issues/186))
- More user-friendly error when no compiler is available ([#185](https://github.com/foundry-rs/compilers/issues/185))

#### Other

- Incorrect Default EVM Version for Solidity Compiler 0.4.21-0.5.4 ([#189](https://github.com/foundry-rs/compilers/issues/189))

## 2024-08-08

### fork-db v0.2.1

#### Bug Fixes

- Fix clippy
- Fix-tests after checking

#### Dependencies

- Merge pull request [#17](https://github.com/foundry-rs/foundry-fork-db/issues/17) from foundry-rs/matt/bump-revm13
- Bump revm 13
- Undo bump version
- Bump version of crate
- Merge bump-revm

#### Documentation

- Docs to functions
- Docs

#### Other

- Merge pull request [#16](https://github.com/foundry-rs/foundry-fork-db/issues/16) from m1stoyanov/patch-1
- Remove the unnecessary result from the helper functions
- Provide helper methods for MemDb data
- Merge pull request [#13](https://github.com/foundry-rs/foundry-fork-db/issues/13) from nkysg/sharedbackend_behaviour
- Update process logic
- Add BlockingMod::Block process
-  add configure for SharedBackend block_in_place or not
- Merge pull request [#10](https://github.com/foundry-rs/foundry-fork-db/issues/10) from Ethanol48/update_state
- Eliminated tmp ETH_RPC
- Added tmp file for testing
- Eliminate redundant code
- Add tests to verify if the data was properly updated
- Added db to test to verify data
- Add minor changes
- Update block hashes
- Typo
- Update address in db
- Update revm
- Merge pull request [#12](https://github.com/foundry-rs/foundry-fork-db/issues/12) from Ethanol48/flush_to_file
- Change to &Path
- Eliminate redundant code
- Merge branch 'main' of https://github.com/Ethanol48/foundry-fork-db into flush_to_file

#### Refactor

- Refactor and storage update
- Refactoring

## 2024-08-01

### compilers v0.10.2

#### Bug Fixes

- Unify logic for ignored warnings ([#179](https://github.com/foundry-rs/compilers/issues/179))
- Remove outdated build infos ([#177](https://github.com/foundry-rs/compilers/issues/177))
- Make remappings resolution more deterministic ([#176](https://github.com/foundry-rs/compilers/issues/176))

#### Features

- Sanitize EVM version for vyper ([#181](https://github.com/foundry-rs/compilers/issues/181))

#### Other

- Update README to link docs and update install instructions ([#180](https://github.com/foundry-rs/compilers/issues/180))

## 2024-07-26

### compilers v0.10.1

#### Bug Fixes

- Better compatibility with older AST ([#175](https://github.com/foundry-rs/compilers/issues/175))

#### Features

- Add Prague evm version ([#166](https://github.com/foundry-rs/compilers/issues/166))

## 2024-07-19

### explorers v0.5.1

#### Dependencies

- Bump compilers ([#55](https://github.com/foundry-rs/block-explorers/issues/55))

## 2024-07-18

### compilers v0.10.0

#### Bug Fixes

- Allow empty modifier body in AST ([#169](https://github.com/foundry-rs/compilers/issues/169))
- Avoid errors when parsing empty sourcemap ([#165](https://github.com/foundry-rs/compilers/issues/165))
- Fix inconsistent trailing slash in remappings ([#49](https://github.com/foundry-rs/compilers/issues/49))

#### Features

- Add `eofVersion` config option ([#174](https://github.com/foundry-rs/compilers/issues/174))
- Allow passing extra cli args to solc + some cleanup ([#171](https://github.com/foundry-rs/compilers/issues/171))

## 2024-07-17

### fork-db v0.2.0

#### Dependencies

- Merge pull request [#8](https://github.com/foundry-rs/foundry-fork-db/issues/8) from foundry-rs/klkvr/bump-revm
- Bump revm
- Merge pull request [#7](https://github.com/foundry-rs/foundry-fork-db/issues/7) from foundry-rs/matt/bump-revm-alloy
- Bump alloy and revm

#### Other

- Formatting
- Add documentation
- Add flush to arbitrary file

## 2024-07-15

### fork-db v0.1.1

#### Dependencies

- Merge pull request [#5](https://github.com/foundry-rs/foundry-fork-db/issues/5) from foundry-rs/matt/bump-msrv
- Bump msrv 79
- Merge pull request [#4](https://github.com/foundry-rs/foundry-fork-db/issues/4) from m1stoyanov/main
- Bump alloy [provider, rpc-types, serde, transport, rpc-client, transport-http] to 0.1.4, alloy-primitives to 0.7.7 and revm to 11.0.0

#### Other

- Remove redundant check
- Update Cargo.toml according to the reviews

## 2024-07-02

### fork-db v0.1.0

#### Bug Fixes

- Clippy
- Cargo deny
- Clippy + fmt
- Tests

#### Miscellaneous Tasks

- Init changelog
- Fix cliff.toml
- Add description

#### Other

- Update naming ([#2](https://github.com/foundry-rs/foundry-fork-db/issues/2))
- Merge pull request [#1](https://github.com/foundry-rs/foundry-fork-db/issues/1) from klkvr/klkvr/init
- DatabaseError -> BackendError
- Initial commit
- Update readme
- Update name
- Initial commit

## 2024-06-29

### compilers v0.9.0

#### Bug Fixes

- Doctests ([#154](https://github.com/foundry-rs/compilers/issues/154))
- [flatten] Small bugs ([#153](https://github.com/foundry-rs/compilers/issues/153))

#### Dependencies

- Cleanup workspace deps ([#158](https://github.com/foundry-rs/compilers/issues/158))

#### Features

- Respect `paths.libraries` for Vyper ([#159](https://github.com/foundry-rs/compilers/issues/159))

#### Miscellaneous Tasks

- Improve stripping file prefixes ([#164](https://github.com/foundry-rs/compilers/issues/164))
- Improve some trace-level logs ([#163](https://github.com/foundry-rs/compilers/issues/163))
- Remove most impl AsRef<str,Path> ([#157](https://github.com/foundry-rs/compilers/issues/157))
- Clarify version cache lock ([#160](https://github.com/foundry-rs/compilers/issues/160))
- Sort derives, derive Eq more ([#161](https://github.com/foundry-rs/compilers/issues/161))
- [meta] Update CODEOWNERS
- Rename foundry-compilers-project into foundry-compilers ([#152](https://github.com/foundry-rs/compilers/issues/152))
- Clippy
- Move lints to workspace ([#149](https://github.com/foundry-rs/compilers/issues/149))
- Remove unused files and workflow ([#148](https://github.com/foundry-rs/compilers/issues/148))

#### Other

- Symlink readme
- Sync workflows

#### Performance

- Cache --version output ([#144](https://github.com/foundry-rs/compilers/issues/144))

#### Refactor

- Unify sources and filtered sources ([#162](https://github.com/foundry-rs/compilers/issues/162))
- [flatten] Move compilation logic into `Flattener` ([#143](https://github.com/foundry-rs/compilers/issues/143))
- Extract artifacts to a separate crate ([#142](https://github.com/foundry-rs/compilers/issues/142))

#### Testing

- Use similar-asserts ([#145](https://github.com/foundry-rs/compilers/issues/145))

### explorers v0.5.0

#### Dependencies

- [deps] Bump compilers 0.9 ([#54](https://github.com/foundry-rs/block-explorers/issues/54))

#### Miscellaneous Tasks

- Fix up manifests
- [meta] Update CODEOWNERS

#### Other

- The EVM version returned by Blockscout is "default"  ([#53](https://github.com/foundry-rs/block-explorers/issues/53))
- Create cache directory if needed ([#52](https://github.com/foundry-rs/block-explorers/issues/52))

## 2024-06-17

### explorers v0.4.1

#### Dependencies

- Bump compilers ([#49](https://github.com/foundry-rs/block-explorers/issues/49))

#### Miscellaneous Tasks

- Use crates alloy ([#50](https://github.com/foundry-rs/block-explorers/issues/50))

## 2024-06-11

### compilers v0.7.0

#### Bug Fixes

- Always fix windows line endings ([#139](https://github.com/foundry-rs/compilers/issues/139))

#### Features

- Track and cache context of each compiler invocation ([#140](https://github.com/foundry-rs/compilers/issues/140))

### explorers v0.4.0

#### Dependencies

- [deps] Bump compilers ([#48](https://github.com/foundry-rs/block-explorers/issues/48))

#### Miscellaneous Tasks

- Sync cliff.toml

## 2024-06-06

### compilers v0.6.2

#### Bug Fixes

- Better tracking of cache entries ([#138](https://github.com/foundry-rs/compilers/issues/138))

## 2024-06-05

### compilers v0.6.1

#### Bug Fixes

- Small sparse output updates ([#137](https://github.com/foundry-rs/compilers/issues/137))
- Version resolution ([#136](https://github.com/foundry-rs/compilers/issues/136))
- Vyper 0.4 support ([#134](https://github.com/foundry-rs/compilers/issues/134))

#### Miscellaneous Tasks

- Sync cliff.toml

#### Refactor

- Sparse output ([#135](https://github.com/foundry-rs/compilers/issues/135))

## 2024-06-03

### compilers v0.6.0

#### Dependencies

- [deps] Bump itertools ([#133](https://github.com/foundry-rs/compilers/issues/133))

#### Features

- Allow multiple languages for compilers ([#128](https://github.com/foundry-rs/compilers/issues/128))

### explorers v0.3.0

#### Dependencies

- Bump compilers ([#47](https://github.com/foundry-rs/block-explorers/issues/47))

## 2024-06-01

### compilers v0.5.2

#### Features

- Make CompactContractBytecodeCow implement Artifact ([#130](https://github.com/foundry-rs/compilers/issues/130))

#### Miscellaneous Tasks

- Clippy ([#132](https://github.com/foundry-rs/compilers/issues/132))

#### Performance

- Reduce size of source map ([#131](https://github.com/foundry-rs/compilers/issues/131))

## 2024-05-23

### compilers v0.5.1

#### Bug Fixes

- Update vyper path resolution logic ([#127](https://github.com/foundry-rs/compilers/issues/127))
- Relax trait bounds ([#126](https://github.com/foundry-rs/compilers/issues/126))

## 2024-05-21

### compilers v0.5.0

#### Features

- Vyper imports parser ([#125](https://github.com/foundry-rs/compilers/issues/125))

#### Miscellaneous Tasks

- Swap generics on `Project` ([#124](https://github.com/foundry-rs/compilers/issues/124))

### explorers v0.2.8

#### Dependencies

- Bump compilers ([#46](https://github.com/foundry-rs/block-explorers/issues/46))

## 2024-05-13

### compilers v0.4.3

#### Bug Fixes

- Re-enable yul settings sanitization ([#122](https://github.com/foundry-rs/compilers/issues/122))

### compilers v0.4.2

#### Bug Fixes

- Do not remove dirty artifacts from disk ([#123](https://github.com/foundry-rs/compilers/issues/123))

## 2024-05-07

### compilers v0.4.1

#### Bug Fixes

- Absolute paths in build info ([#121](https://github.com/foundry-rs/compilers/issues/121))

#### Features

- Add a few Solc install helpers back ([#120](https://github.com/foundry-rs/compilers/issues/120))

## 2024-05-03

### compilers v0.4.0

#### Features

- Compiler abstraction ([#115](https://github.com/foundry-rs/compilers/issues/115))

### explorers v0.2.7

#### Bug Fixes

- [blob-explorers] `alloy-serde` usage ([#45](https://github.com/foundry-rs/block-explorers/issues/45))

#### Dependencies

- Bump foundry-compilers ([#44](https://github.com/foundry-rs/block-explorers/issues/44))

#### Features

- Add blob-explorer crate ([#42](https://github.com/foundry-rs/block-explorers/issues/42))

#### Miscellaneous Tasks

- Convert to workspace ([#41](https://github.com/foundry-rs/block-explorers/issues/41))

## 2024-04-30

### compilers v0.3.20

#### Bug Fixes

- Short-circuit symlink cycle ([#117](https://github.com/foundry-rs/compilers/issues/117))
- Add checks for != root folder ([#116](https://github.com/foundry-rs/compilers/issues/116))

## 2024-04-22

### compilers v0.3.19

#### Bug Fixes

- Remove `simpleCounterForLoopUncheckedIncrement` from `--ir-minimum` ([#114](https://github.com/foundry-rs/compilers/issues/114))
- Add YulCase and YulTypedName to NodeType ([#111](https://github.com/foundry-rs/compilers/issues/111))
- Use serde default for optimizer ([#109](https://github.com/foundry-rs/compilers/issues/109))
- Replace line endings on Windows to enforce deterministic metadata ([#108](https://github.com/foundry-rs/compilers/issues/108))

## 2024-04-19

### compilers v0.3.18

#### Miscellaneous Tasks

- Warn unused ([#106](https://github.com/foundry-rs/compilers/issues/106))

#### Other

- Update yansi to 1.0 ([#107](https://github.com/foundry-rs/compilers/issues/107))

## 2024-04-17

### compilers v0.3.17

#### Bug Fixes

- Dirty files detection ([#105](https://github.com/foundry-rs/compilers/issues/105))

#### Features

- Additional helpers for contract name -> path lookup ([#103](https://github.com/foundry-rs/compilers/issues/103))

### compilers v0.3.16

#### Bug Fixes

- Invalidate cache for out-of-scope entries ([#104](https://github.com/foundry-rs/compilers/issues/104))

#### Features

- Optimization field (simpleCounterForLoopUncheckedIncrement) ([#100](https://github.com/foundry-rs/compilers/issues/100))

#### Miscellaneous Tasks

- Remove main fn ([#101](https://github.com/foundry-rs/compilers/issues/101))

## 2024-04-15

### explorers v0.2.6

#### Other

- Etherscan cache is constantly invalidated ([#40](https://github.com/foundry-rs/block-explorers/issues/40))

## 2024-04-12

### compilers v0.3.15

#### Dependencies

- [deps] Bump svm to 0.5 ([#97](https://github.com/foundry-rs/compilers/issues/97))

#### Miscellaneous Tasks

- Derive `Clone` for `Project` ([#98](https://github.com/foundry-rs/compilers/issues/98))

## 2024-04-03

### compilers v0.3.14

#### Bug Fixes

- Set evmversion::cancun as default ([#94](https://github.com/foundry-rs/compilers/issues/94))

#### Dependencies

- Bump alloy-core ([#96](https://github.com/foundry-rs/compilers/issues/96))

### explorers v0.2.5

#### Dependencies

- Bump alloy-core ([#39](https://github.com/foundry-rs/block-explorers/issues/39))

## 2024-03-29

### explorers v0.2.4

#### Bug Fixes

- Avoid setting extension when writing source tree to disk ([#32](https://github.com/foundry-rs/block-explorers/issues/32))

#### Dependencies

- [deps] Bump reqwest to 0.12 ([#37](https://github.com/foundry-rs/block-explorers/issues/37))

#### Miscellaneous Tasks

- Remove unused imports ([#33](https://github.com/foundry-rs/block-explorers/issues/33))

#### Other

- Chain_id param in etherscan query req ([#36](https://github.com/foundry-rs/block-explorers/issues/36))
- Add concurrency ([#34](https://github.com/foundry-rs/block-explorers/issues/34))

## 2024-03-18

### compilers v0.3.13

#### Miscellaneous Tasks

- Svm04 ([#93](https://github.com/foundry-rs/compilers/issues/93))

### compilers v0.3.12

#### Miscellaneous Tasks

- Update svm ([#92](https://github.com/foundry-rs/compilers/issues/92))

## 2024-03-13

### compilers v0.3.11

#### Refactor

- Caching logic ([#90](https://github.com/foundry-rs/compilers/issues/90))

## 2024-03-11

### compilers v0.3.10

#### Features

- Use cached artifacts if solc config is almost the same ([#87](https://github.com/foundry-rs/compilers/issues/87))

#### Other

- Helper for `OutputSelection` ([#89](https://github.com/foundry-rs/compilers/issues/89))
- Add `CARGO_TERM_COLOR` env ([#86](https://github.com/foundry-rs/compilers/issues/86))

#### Refactor

- Extra files logic ([#88](https://github.com/foundry-rs/compilers/issues/88))

## 2024-02-22

### compilers v0.3.9

#### Bug Fixes

- Account for Solc inexplicably not formatting the message ([#85](https://github.com/foundry-rs/compilers/issues/85))

### compilers v0.3.8

#### Bug Fixes

- Always treat errors as error ([#84](https://github.com/foundry-rs/compilers/issues/84))
- Make solc emit ir with extra_output_files=ir ([#82](https://github.com/foundry-rs/compilers/issues/82))

#### Miscellaneous Tasks

- Use Path::new instead of PathBuf::from ([#83](https://github.com/foundry-rs/compilers/issues/83))

## 2024-02-20

### compilers v0.3.7

#### Bug Fixes

- Don't bother formatting old solc errors ([#81](https://github.com/foundry-rs/compilers/issues/81))
- Empty error message formatting ([#77](https://github.com/foundry-rs/compilers/issues/77))

#### Miscellaneous Tasks

- Print compiler input as JSON in traces ([#79](https://github.com/foundry-rs/compilers/issues/79))
- Remove unused imports ([#80](https://github.com/foundry-rs/compilers/issues/80))
- Reduce trace output ([#78](https://github.com/foundry-rs/compilers/issues/78))

## 2024-02-13

### compilers v0.3.6

#### Other

- Small flattener features ([#75](https://github.com/foundry-rs/compilers/issues/75))

## 2024-02-10

### compilers v0.3.5

#### Bug Fixes

- Fix `DoWhileStatement` AST ([#74](https://github.com/foundry-rs/compilers/issues/74))

## 2024-02-09

### compilers v0.3.4

#### Dependencies

- Option to ignore warnings from dependencies in foundry.toml ([#69](https://github.com/foundry-rs/compilers/issues/69))

## 2024-02-08

### compilers v0.3.3

#### Other

- Helper method for `Libraries` ([#72](https://github.com/foundry-rs/compilers/issues/72))

## 2024-02-07

### compilers v0.3.2

#### Bug Fixes

- Also cleanup build info dir ([#71](https://github.com/foundry-rs/compilers/issues/71))

## 2024-02-02

### compilers v0.3.1

#### Other

- Flatten fix ([#68](https://github.com/foundry-rs/compilers/issues/68))

## 2024-01-31

### compilers v0.3.0

#### Dependencies

- Remove unnecessary dependencies ([#65](https://github.com/foundry-rs/compilers/issues/65))
- Bump to 0.8.24 in tests ([#59](https://github.com/foundry-rs/compilers/issues/59))

#### Miscellaneous Tasks

- Enable some lints ([#64](https://github.com/foundry-rs/compilers/issues/64))
- Remove wasm cfgs ([#61](https://github.com/foundry-rs/compilers/issues/61))
- Add more tracing around spawning Solc ([#57](https://github.com/foundry-rs/compilers/issues/57))
- Rename output to into_output ([#56](https://github.com/foundry-rs/compilers/issues/56))
- Add some tracing ([#55](https://github.com/foundry-rs/compilers/issues/55))

#### Other

- Flatten fixes ([#63](https://github.com/foundry-rs/compilers/issues/63))
- Update actions@checkout ([#66](https://github.com/foundry-rs/compilers/issues/66))
- Add concurrency to ci.yml ([#62](https://github.com/foundry-rs/compilers/issues/62))
- Fix tests name ([#60](https://github.com/foundry-rs/compilers/issues/60))

#### Refactor

- Rewrite examples without wrapper functions and with no_run ([#58](https://github.com/foundry-rs/compilers/issues/58))

#### Testing

- Ignore old solc version test ([#67](https://github.com/foundry-rs/compilers/issues/67))

### explorers v0.2.3

#### Dependencies

- Bump foundry-compilers 0.3 ([#30](https://github.com/foundry-rs/block-explorers/issues/30))

## 2024-01-29

### compilers v0.2.5

#### Miscellaneous Tasks

- [clippy] Make clippy happy ([#54](https://github.com/foundry-rs/compilers/issues/54))

#### Other

- New flattening impl ([#52](https://github.com/foundry-rs/compilers/issues/52))

## 2024-01-27

### compilers v0.2.4

#### Dependencies

- Bump svm builds ([#53](https://github.com/foundry-rs/compilers/issues/53))

## 2024-01-26

### compilers v0.2.3

#### Features

- Add EVM version Cancun ([#51](https://github.com/foundry-rs/compilers/issues/51))

#### Miscellaneous Tasks

- Add unreleased section to cliff.toml
- Add error severity fn helpers ([#48](https://github.com/foundry-rs/compilers/issues/48))

#### Other

- Small fixes to typed AST ([#50](https://github.com/foundry-rs/compilers/issues/50))

### explorers v0.2.2

#### Miscellaneous Tasks

- Include value in serde error ([#29](https://github.com/foundry-rs/block-explorers/issues/29))
- Add unreleased section to cliff.toml
- Fix cliff, update CHANGELOG

## 2024-01-19

### compilers v0.2.2

#### Other

- Rewrite dirty files discovery ([#45](https://github.com/foundry-rs/compilers/issues/45))

## 2024-01-18

### explorers v0.2.1

#### Features

- Add viaIR to VerifyContract ([#28](https://github.com/foundry-rs/block-explorers/issues/28))

## 2024-01-10

### compilers v0.2.1

#### Miscellaneous Tasks

- Exclude useless directories
- Exclude useless directories

### compilers v0.2.0

#### Dependencies

- [deps] Bump alloy ([#42](https://github.com/foundry-rs/compilers/issues/42))

### explorers v0.2.0

#### Bug Fixes

- Exclude

#### Dependencies

- [deps] Bump compilers
- [deps] Bump alloys ([#27](https://github.com/foundry-rs/block-explorers/issues/27))

#### Miscellaneous Tasks

- Exclude useless directories
- Update cliff link
- Add CHANGELOG.md scripts ([#26](https://github.com/foundry-rs/block-explorers/issues/26))

## 2024-01-06

### compilers v0.1.4

#### Bug Fixes

- Account for unicode width in error syntax highlighting ([#40](https://github.com/foundry-rs/compilers/issues/40))

## 2024-01-05

### compilers v0.1.3

#### Features

- Add evmVersion to settings ([#41](https://github.com/foundry-rs/compilers/issues/41))
- Use Box<dyn> in sparse functions ([#39](https://github.com/foundry-rs/compilers/issues/39))

#### Miscellaneous Tasks

- Clippies and such ([#38](https://github.com/foundry-rs/compilers/issues/38))
- Purge tracing imports ([#37](https://github.com/foundry-rs/compilers/issues/37))

### explorers v0.1.3

#### Bug Fixes

- Dont force trailing url slash ([#25](https://github.com/foundry-rs/block-explorers/issues/25))
- Fix deserialization error resulting from Blockscout omitting "OptimizationRuns" field when optimization was not used ([#23](https://github.com/foundry-rs/block-explorers/issues/23))
- Fix deserialization failure when fetching contract source_code from blockscout ([#22](https://github.com/foundry-rs/block-explorers/issues/22))

#### Other

- Add `getcontractcreation` binding ([#24](https://github.com/foundry-rs/block-explorers/issues/24))

## 2023-12-29

### compilers v0.1.2

#### Bug Fixes

- Create valid Standard JSON to verify for projects with symlinks ([#35](https://github.com/foundry-rs/compilers/issues/35))
- Create verifiable Standard JSON for projects with external files ([#36](https://github.com/foundry-rs/compilers/issues/36))

#### Features

- Add more getter methods to bytecode structs ([#30](https://github.com/foundry-rs/compilers/issues/30))

#### Miscellaneous Tasks

- Add `set_compiled_artifacts` to ProjectCompileOutput impl ([#33](https://github.com/foundry-rs/compilers/issues/33))

#### Other

- Trim test matrix ([#32](https://github.com/foundry-rs/compilers/issues/32))

#### Styling

- Update rustfmt config ([#31](https://github.com/foundry-rs/compilers/issues/31))

## 2023-12-08

### explorers v0.1.2

#### Bug Fixes

- Sanitize all source entries ([#19](https://github.com/foundry-rs/block-explorers/issues/19))

## 2023-11-23

### compilers v0.1.1

#### Bug Fixes

- Default Solidity language string ([#28](https://github.com/foundry-rs/compilers/issues/28))
- [`ci`] Put flags inside matrix correctly ([#20](https://github.com/foundry-rs/compilers/issues/20))

#### Dependencies

- Bump Alloy
- Bump solc ([#21](https://github.com/foundry-rs/compilers/issues/21))

#### Miscellaneous Tasks

- [meta] Update CODEOWNERS
- Remove LosslessAbi ([#27](https://github.com/foundry-rs/compilers/issues/27))

#### Performance

- Don't prettify json when not necessary ([#24](https://github.com/foundry-rs/compilers/issues/24))

#### Styling

- Toml
- More test in report/compiler.rs and Default trait for CompilerInput ([#19](https://github.com/foundry-rs/compilers/issues/19))

### explorers v0.1.1

#### Dependencies

- Bump Alloy

#### Miscellaneous Tasks

- [meta] Add CODEOWNERS

## 2023-11-15

### explorers v0.1.0

#### Bug Fixes

- Add licensing ([#4](https://github.com/foundry-rs/block-explorers/issues/4))
- [features] Remove ethers-solc for foundry-compilers ([#3](https://github.com/foundry-rs/block-explorers/issues/3))

#### Dependencies

- Bump ethers ([#9](https://github.com/foundry-rs/block-explorers/issues/9))

#### Documentation

- Add CHANGELOG.md

#### Features

- Remove Ethers ([#14](https://github.com/foundry-rs/block-explorers/issues/14))
- Repo improvements ([#11](https://github.com/foundry-rs/block-explorers/issues/11))
- Alloy migration ([#2](https://github.com/foundry-rs/block-explorers/issues/2))
- [`CI`] Enable ci/cd ([#1](https://github.com/foundry-rs/block-explorers/issues/1))
- Repo init

#### Miscellaneous Tasks

- [meta] Update configs ([#15](https://github.com/foundry-rs/block-explorers/issues/15))
- Remove RawAbi and LosslessAbi usage ([#12](https://github.com/foundry-rs/block-explorers/issues/12))
- Enable more lints ([#13](https://github.com/foundry-rs/block-explorers/issues/13))
- Remove default feats from openssl ([#7](https://github.com/foundry-rs/block-explorers/issues/7))
- Patch ethers to be in sync w/ foundry ([#6](https://github.com/foundry-rs/block-explorers/issues/6))
- Clippy ([#5](https://github.com/foundry-rs/block-explorers/issues/5))

#### Other

- Update README.md

#### Styling

- Update rustfmt config ([#16](https://github.com/foundry-rs/block-explorers/issues/16))

## 2023-11-07

### compilers v0.1.0

#### Bug Fixes

- Add changelog.sh ([#18](https://github.com/foundry-rs/compilers/issues/18))

#### Dependencies

- Bump solang parser to 0.3.3 ([#11](https://github.com/foundry-rs/compilers/issues/11))
- Remove unneeded deps ([#4](https://github.com/foundry-rs/compilers/issues/4))

#### Features

- [`ci`] Add unused deps workflow ([#15](https://github.com/foundry-rs/compilers/issues/15))
- Migration to Alloy ([#3](https://github.com/foundry-rs/compilers/issues/3))
- [`ci`] Add deny deps CI ([#6](https://github.com/foundry-rs/compilers/issues/6))
- [`ci`] Add & enable ci/cd ([#1](https://github.com/foundry-rs/compilers/issues/1))
- Move ethers-solc into foundry-compilers

#### Miscellaneous Tasks

- Add missing cargo.toml fields + changelog tag ([#17](https://github.com/foundry-rs/compilers/issues/17))
- Add missing telegram url ([#14](https://github.com/foundry-rs/compilers/issues/14))
- Remove alloy-dyn-abi as its an unused dep ([#12](https://github.com/foundry-rs/compilers/issues/12))
- Make clippy happy ([#10](https://github.com/foundry-rs/compilers/issues/10))
- Run ci on main ([#5](https://github.com/foundry-rs/compilers/issues/5))
- Add more files to gitignore ([#2](https://github.com/foundry-rs/compilers/issues/2))
- Correct readme

#### Other

- Repo improvements ([#13](https://github.com/foundry-rs/compilers/issues/13))
