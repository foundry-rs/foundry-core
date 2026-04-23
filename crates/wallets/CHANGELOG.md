# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0](https://github.com/foundry-rs/foundry-core/releases/tag/0.1.0) - 2026-04-23

### Bug Fixes

- Restore changelogs from main, confirm before actions in release script
- Fix version
- [wallets] Dev origin url without https + 'none' frame-src CSP ([#19](https://github.com/foundry-rs/foundry-core/issues/19))
- [wallets] Dev origin url without https + 'none' frame-src CSP
- Fmt
- Remove static deps to foundry
- [wallets] Browser wallet CLI help heading formatting ([#13876](https://github.com/foundry-rs/foundry-core/issues/13876))
- [deps] Bump to Foundry browser wallet version 0.2.0 ([#13890](https://github.com/foundry-rs/foundry-core/issues/13890))
- [wallets] Use turnkey_unsupported() instead of hardcoded error ([#13535](https://github.com/foundry-rs/foundry-core/issues/13535))
- [script] Set msg.sender from TURNKEY_ADDRESS when using --turnkey ([#13149](https://github.com/foundry-rs/foundry-core/issues/13149))
- [wallets] Prevent duplicate Trezor wallets when using --hd-paths ([#12214](https://github.com/foundry-rs/foundry-core/issues/12214))
- Fix GCP env var names ([#12129](https://github.com/foundry-rs/foundry-core/issues/12129))
- Improve error handling in available_senders with logging and deduplication ([#12011](https://github.com/foundry-rs/foundry-core/issues/12011))
- Split on whitespace in mnemonic parsing ([#11257](https://github.com/foundry-rs/foundry-core/issues/11257))
- [docs] Correct --unlocked flag documentation for RPC transaction signing ([#10929](https://github.com/foundry-rs/foundry-core/issues/10929))
- Force install default crypto provider ([#10327](https://github.com/foundry-rs/foundry-core/issues/10327))
- Panic in WalletSigner::from_private_key ([#8052](https://github.com/foundry-rs/foundry-core/issues/8052))
- Re-enable aws-config default-features ([#8058](https://github.com/foundry-rs/foundry-core/issues/8058))
- Use B256::try_from for pk ([#7871](https://github.com/foundry-rs/foundry-core/issues/7871))
- Enable eip712 for all signers ([#7854](https://github.com/foundry-rs/foundry-core/issues/7854))
- [forge] Prefer --from if specified for `cast call` ([#7218](https://github.com/foundry-rs/foundry-core/issues/7218))
- [chisel] Min and max for all types ([#7192](https://github.com/foundry-rs/foundry-core/issues/7192))

### Dependencies

- [wallets] Bump foundry-browser-wallet to v0.5.0 ([#20](https://github.com/foundry-rs/foundry-core/issues/20))
- Remove duplicate dev-dependencies ([#13597](https://github.com/foundry-rs/foundry-core/issues/13597))
- [deps] Reuse aws and gcp package from alloy ([#11573](https://github.com/foundry-rs/foundry-core/issues/11573))
- Update dependencies ([#11007](https://github.com/foundry-rs/foundry-core/issues/11007))
- Bump to rust edition 2024 ([#10802](https://github.com/foundry-rs/foundry-core/issues/10802))
- [`revm`: step 2] Bump `alloy` + `revm` + `alloy-evm` + other deps to latest ([#10454](https://github.com/foundry-rs/foundry-core/issues/10454))
- [deps] Bump alloy 0.11 ([#9798](https://github.com/foundry-rs/foundry-core/issues/9798))
- [deps] Bump alloy 0.6.4 ([#9280](https://github.com/foundry-rs/foundry-core/issues/9280))
- [deps] Prefer soft pinning on minor version for dependencies ([#9269](https://github.com/foundry-rs/foundry-core/issues/9269))
- [deps] Bump alloy-core 0.8.6 ([#9045](https://github.com/foundry-rs/foundry-core/issues/9045))
- Bump alloy-core deps + revm ([#8988](https://github.com/foundry-rs/foundry-core/issues/8988))
- [deps] Move more deps to workspace ([#8192](https://github.com/foundry-rs/foundry-core/issues/8192))
- [deps] Bump alloy, revm ([#8177](https://github.com/foundry-rs/foundry-core/issues/8177))
- Support GCP KMS Signer ([#8096](https://github.com/foundry-rs/foundry-core/issues/8096))
- Upgrade to latest version of Alloy and port Anvil tests ([#7701](https://github.com/foundry-rs/foundry-core/issues/7701))
- Bump alloy to use `get_receipt` hotfix ([#7772](https://github.com/foundry-rs/foundry-core/issues/7772))
- [wip] feat: provider alloy migration ([#7106](https://github.com/foundry-rs/foundry-core/issues/7106))

### Documentation

- Add README for each crate ([#11](https://github.com/foundry-rs/foundry-core/issues/11))
- Add README for each crate
- Clarify keystore path should point to a filename ([#9004](https://github.com/foundry-rs/foundry-core/issues/9004))
- [`forge script`] Improve `Mac Mismatch` error referring to failure to decrypt of keystore ([#8572](https://github.com/foundry-rs/foundry-core/issues/8572))

### Features

- Add local interactive release flow ([#25](https://github.com/foundry-rs/foundry-core/issues/25))
- [wallets] Feature-gate browser and tempo signers ([#22](https://github.com/foundry-rs/foundry-core/issues/22))
- Add SQLite channel store ([#17](https://github.com/foundry-rs/foundry-core/issues/17))
- Add SQLite channel store
- [wallets] Add `--tempo.access-key` and `--tempo.root-account`  ([#14201](https://github.com/foundry-rs/foundry-core/issues/14201))
- [common] `FoundryTransactionBuilder::sign_with_access_key` method ([#14120](https://github.com/foundry-rs/foundry-core/issues/14120))
- Tempo wallet access key support for cast ([#13909](https://github.com/foundry-rs/foundry-core/issues/13909))
- [script] Generic `BundledState` impl ([#13825](https://github.com/foundry-rs/foundry-core/issues/13825))
- [cheatcodes] Bubble-up `Network` generic to `Wallets` ([#13768](https://github.com/foundry-rs/foundry-core/issues/13768))
- [wallets] `MultiWallet` generic `Network` ([#13648](https://github.com/foundry-rs/foundry-core/issues/13648))
- [wallets] Introduce `BrowserWalletOpts` ([#13602](https://github.com/foundry-rs/foundry-core/issues/13602))
- [forge] Add browser wallet support for `forge script` ([#12952](https://github.com/foundry-rs/foundry-core/issues/12952))
- [cast] Add tempo tx construction support ([#12973](https://github.com/foundry-rs/foundry-core/issues/12973))
- Feat!(`forge script`): add `--interactive` flag for deploying with a single keypair ([#12608](https://github.com/foundry-rs/foundry-core/issues/12608))
- Browser wallet ([#12302](https://github.com/foundry-rs/foundry-core/issues/12302))
- [wallets] Add Turnkey signer support ([#12026](https://github.com/foundry-rs/foundry-core/issues/12026))
- Feat(cast) more descriptive errors for `gcp` & `aws` signers ([#11248](https://github.com/foundry-rs/foundry-core/issues/11248))
- [cast] `cast wallet sign-auth` + `cast send --auth` ([#8683](https://github.com/foundry-rs/foundry-core/issues/8683))
- Extract ABIs and formatting code into separate crates ([#8240](https://github.com/foundry-rs/foundry-core/issues/8240))
- Remove most of ethers ([#7861](https://github.com/foundry-rs/foundry-core/issues/7861))
- Feat(`cast wallet list`) issue [#6958](https://github.com/foundry-rs/foundry-core/issues/6958): Include HW wallets in cast wallet ls ([#7123](https://github.com/foundry-rs/foundry-core/issues/7123))
- [cast] Add `wallet sign --no-hash` ([#7180](https://github.com/foundry-rs/foundry-core/issues/7180))

### Miscellaneous Tasks

- Add initial wallets CHANGELOG.md ([#28](https://github.com/foundry-rs/foundry-core/issues/28))
- Add wallets CHANGELOG.md
- Replace CI OIDC release with local release flow
- Take exact version in Makefile release targets, add wallets description
- Per-crate cliff.toml and release.toml for independent releases
- Move top-level test fixtures into wallets crate
- [wallets] Remove unused `Signer` impls for `BrowserSigner` ([#13657](https://github.com/foundry-rs/foundry-core/issues/13657))
- [wallets] Remove `NetworkWallet<FoundryNetwork>` impl for `WalletSigner` ([#13343](https://github.com/foundry-rs/foundry-core/issues/13343))
- Remove feature(doc_auto_cfg) ([#11852](https://github.com/foundry-rs/foundry-core/issues/11852))
- [docs] Mention keystore in wallet error message, improve readability ([#11405](https://github.com/foundry-rs/foundry-core/issues/11405))
- Aggregate PRs ([#11384](https://github.com/foundry-rs/foundry-core/issues/11384))
- [wallets] Improve error message for signer instantiation failure ([#10646](https://github.com/foundry-rs/foundry-core/issues/10646))
- [all] Replace 0x prefix from_str(...).unwrap() with macros ([#10222](https://github.com/foundry-rs/foundry-core/issues/10222))
- Remove rustls/openssl features ([#9824](https://github.com/foundry-rs/foundry-core/issues/9824))
- Fix clippy ([#9790](https://github.com/foundry-rs/foundry-core/issues/9790))
- Fix base gas limit test and clippy ([#8961](https://github.com/foundry-rs/foundry-core/issues/8961))
- Add and use workspace.lints ([#8067](https://github.com/foundry-rs/foundry-core/issues/8067))
- Hide aws flags when not enabled ([#7979](https://github.com/foundry-rs/foundry-core/issues/7979))
- Make aws-kms signer support optional ([#7976](https://github.com/foundry-rs/foundry-core/issues/7976))
- Remove `cast bind` ([#7887](https://github.com/foundry-rs/foundry-core/issues/7887))
- Stop using RuntimeOrHandle ([#7860](https://github.com/foundry-rs/foundry-core/issues/7860))
- [cli] Fix clap deprecated warnings ([#7274](https://github.com/foundry-rs/foundry-core/issues/7274))

### Other

- Merge branch 'main' into steven/add-channel-db
- Merge branch 'main' into zerosnacks/harden-makefile-cleanup
- Imports
- Import rustqlite top
- Merge branch 'main' into zerosnacks/old-repo-links
- Enable `missing_const_for_fn` (autofix) ([#14297](https://github.com/foundry-rs/foundry-core/issues/14297))
- Warn on `redundant_else` ([#14088](https://github.com/foundry-rs/foundry-core/issues/14088))
- Delegate TxSigner::address() to Signer::address() ([#12948](https://github.com/foundry-rs/foundry-core/issues/12948))
- Improve remote wallet --help commands ([#11891](https://github.com/foundry-rs/foundry-core/issues/11891))
- Remove the --froms flag ([#11099](https://github.com/foundry-rs/foundry-core/issues/11099))
- Support the `gcp` option in `cast wallet list` ([#8232](https://github.com/foundry-rs/foundry-core/issues/8232))
- Alloy update ([#8660](https://github.com/foundry-rs/foundry-core/issues/8660))
- Init foundry-wallets ([#7086](https://github.com/foundry-rs/foundry-core/issues/7086))

### Performance

- Reduce memory usage by boxing large error variants in wallet operations ([#11928](https://github.com/foundry-rs/foundry-core/issues/11928))

### Refactor

- [script] Extract `BrowserSigner` from `MultiWallet` ([#13839](https://github.com/foundry-rs/foundry-core/issues/13839))
- [wallets] Extract `Browser` from `WalletSigner` ([#13613](https://github.com/foundry-rs/foundry-core/issues/13613))
- [wallets] Browser wallet generic `Network` ([#13550](https://github.com/foundry-rs/foundry-core/issues/13550))
- [wallets] Reduce iterator cloning ([#11931](https://github.com/foundry-rs/foundry-core/issues/11931))
- Wallet management ([#7141](https://github.com/foundry-rs/foundry-core/issues/7141))

### Styling

- Warn on `if_not_else` (autofix) ([#14092](https://github.com/foundry-rs/foundry-core/issues/14092))

### Testing

- Misc test improvements ([#9812](https://github.com/foundry-rs/foundry-core/issues/9812))
- Add test for pk parsing ([#8366](https://github.com/foundry-rs/foundry-core/issues/8366))

<!-- generated by git-cliff -->
