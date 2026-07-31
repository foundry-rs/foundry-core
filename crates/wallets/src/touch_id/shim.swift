// C-ABI shim over CryptoKit's Secure Enclave API for the `touch-id` feature.
//
// Persisting a Secure Enclave key without keychain entitlements is only possible
// through CryptoKit's `dataRepresentation` (an enclave-encrypted, device-bound
// blob), which has no C or Objective-C surface — hence this Swift file, compiled
// and linked by this crate's build script.
//
// Scheme (the `age-plugin-se` pattern): an enclave-resident P-256 key wraps a
// secret via ECIES (ephemeral P-256 ECDH + HKDF-SHA256 + ChaChaPoly). Wrapping
// uses only the public key and never prompts; unwrapping authenticates the user
// and performs the ECDH inside the enclave, which enforces the key's
// access-control policy.
//
// All functions return `statusOk` (0) on success. On failure they return one of
// the `status*` codes below and, when the out parameters are provided, place a
// malloc'd UTF-8 error message in them. The codes distinguish user cancellation
// from environmental unavailability from an unusable enrollment so the Rust
// side can decide between aborting and falling back to the password prompt.
// Output buffers are malloc'd and must be released with `foundry_se_free`.

import CryptoKit
import Darwin
import Foundation
import LocalAuthentication
import Security

private let hkdfInfo = Data("foundry-touch-id-v1".utf8)
private let x963PublicKeyLen = 65
private let chaChaPolyOverheadLen = 12 + 16  // nonce + tag

// Status codes shared with the `status` module in `mod.rs`.
private let statusOk: Int32 = 0
private let statusFailure: Int32 = 1
private let statusCanceled: Int32 = 2
private let statusUnavailable: Int32 = 3
private let statusInvalidated: Int32 = 4
private let statusInvalidData: Int32 = 5
private let statusLockedOut: Int32 = 6

// Access-control policies shared with `Policy::raw` in `mod.rs`.
private let policyDeviceOnly: Int32 = 0
private let policyUserPresence: Int32 = 1
private let policyCurrentBiometry: Int32 = 2

/// A classified failure carrying the status code reported over the FFI.
private struct ShimError: Error, CustomStringConvertible {
    let status: Int32
    let message: String
    var description: String { message }
}

private func setOut(
    _ bytes: Data,
    _ outPtr: UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>,
    _ outLen: UnsafeMutablePointer<Int>
) {
    // `!` traps deterministically on allocation failure, even under -O.
    let buf = malloc(bytes.count)!.assumingMemoryBound(to: UInt8.self)
    bytes.copyBytes(to: buf, count: bytes.count)
    outPtr.pointee = buf
    outLen.pointee = bytes.count
}

private func fail(
    _ status: Int32,
    _ message: String,
    _ outPtr: UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>,
    _ outLen: UnsafeMutablePointer<Int>
) -> Int32 {
    setOut(Data(message.utf8), outPtr, outLen)
    return status
}

private func accessControl(policy: Int32) throws -> SecAccessControl {
    var flags: SecAccessControlCreateFlags = [.privateKeyUsage]
    switch policy {
    // Test-only; `Policy::DeviceOnly` is `#[cfg(test)]` on the Rust side.
    case policyDeviceOnly: break
    case policyUserPresence: flags.insert(.userPresence)
    case policyCurrentBiometry: flags.insert(.biometryCurrentSet)
    // Fail closed: an unknown policy must never silently create a key with
    // weaker protection than the caller intended.
    default:
        throw ShimError(status: statusFailure, message: "unknown access-control policy \(policy)")
    }
    var error: Unmanaged<CFError>?
    guard
        let ac = SecAccessControlCreateWithFlags(
            kCFAllocatorDefault, kSecAttrAccessibleWhenUnlockedThisDeviceOnly, flags, &error)
    else {
        throw error!.takeRetainedValue() as Error
    }
    return ac
}

/// Maps a LocalAuthentication/Security failure onto a shim status.
private func classify(_ error: Error, policy: Int32) -> ShimError {
    let ns = error as NSError
    if ns.domain == LAError.errorDomain, let code = LAError.Code(rawValue: ns.code) {
        switch code {
        case .userCancel, .appCancel, .systemCancel:
            return ShimError(status: statusCanceled, message: "authentication was canceled")
        case .authenticationFailed:
            return ShimError(status: statusFailure, message: "authentication failed")
        case .biometryLockout:
            return ShimError(status: statusLockedOut, message: ns.localizedDescription)
        case .biometryNotEnrolled where policy == policyCurrentBiometry:
            // Under `.biometryCurrentSet`, removing the enrolled biometrics
            // permanently invalidates the wrap key: "no biometrics enrolled"
            // is the invalidation itself, not a recoverable environment.
            return ShimError(
                status: statusInvalidated,
                message: "biometric enrollment changed since this keystore was enrolled")
        case .passcodeNotSet, .biometryNotAvailable, .biometryNotEnrolled, .notInteractive:
            return ShimError(status: statusUnavailable, message: ns.localizedDescription)
        default: break
        }
    }
    if ns.domain == NSOSStatusErrorDomain {
        switch Int32(ns.code) {
        case errSecUserCanceled:
            return ShimError(status: statusCanceled, message: "authentication was canceled")
        case errSecInteractionNotAllowed:
            return ShimError(status: statusUnavailable, message: ns.localizedDescription)
        default: break
        }
    }
    return ShimError(status: statusFailure, message: "\(error)")
}

/// Authenticates the user on `context` under the LocalAuthentication policy
/// matching the key's access-control policy, so the subsequent enclave
/// operation consumes that fresh authentication instead of driving its own
/// prompt. Doing the evaluation explicitly is what makes failures precise:
/// cancellation and unavailability surface here as typed `LAError`s, and
/// whatever the enclave still rejects afterwards is key trouble, not user
/// trouble.
private func preauthenticate(_ context: LAContext, policy: Int32, reason: String) throws {
    let laPolicy: LAPolicy
    switch policy {
    // Test-only: nothing to evaluate.
    case policyDeviceOnly: return
    case policyUserPresence: laPolicy = .deviceOwnerAuthentication
    case policyCurrentBiometry: laPolicy = .deviceOwnerAuthenticationWithBiometrics
    default:
        throw ShimError(status: statusFailure, message: "unknown access-control policy \(policy)")
    }

    var check: NSError?
    guard context.canEvaluatePolicy(laPolicy, error: &check) else {
        guard let check else {
            throw ShimError(
                status: statusFailure, message: "policy evaluation failed without an error")
        }
        throw classify(check, policy: policy)
    }

    let done = DispatchSemaphore(value: 0)
    var failure: Error?
    context.evaluatePolicy(laPolicy, localizedReason: reason) { success, error in
        if !success {
            failure = error ?? ShimError(status: statusFailure, message: "authentication failed")
        }
        done.signal()
    }
    done.wait()
    if let failure {
        throw classify(failure, policy: policy)
    }
}

@_cdecl("foundry_se_available")
public func foundrySeAvailable() -> Int32 {
    SecureEnclave.isAvailable ? 1 : 0
}

@_cdecl("foundry_se_create")
public func foundrySeCreate(
    _ policy: Int32,
    _ outPtr: UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>,
    _ outLen: UnsafeMutablePointer<Int>
) -> Int32 {
    guard SecureEnclave.isAvailable else {
        return fail(
            statusUnavailable, "Secure Enclave is not available on this machine", outPtr, outLen)
    }
    do {
        let key = try SecureEnclave.P256.KeyAgreement.PrivateKey(
            accessControl: accessControl(policy: policy))
        setOut(key.dataRepresentation, outPtr, outLen)
        return statusOk
    } catch let error as ShimError {
        return fail(error.status, error.message, outPtr, outLen)
    } catch {
        return fail(
            statusFailure, "failed to create Secure Enclave key: \(error)", outPtr, outLen)
    }
}

@_cdecl("foundry_se_wrap")
public func foundrySeWrap(
    _ blobPtr: UnsafePointer<UInt8>, _ blobLen: Int,
    _ plainPtr: UnsafePointer<UInt8>, _ plainLen: Int,
    _ outPtr: UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>,
    _ outLen: UnsafeMutablePointer<Int>
) -> Int32 {
    do {
        let key = try SecureEnclave.P256.KeyAgreement.PrivateKey(
            dataRepresentation: Data(bytes: blobPtr, count: blobLen))
        let recipientPub = key.publicKey
        let ephemeral = P256.KeyAgreement.PrivateKey()
        let shared = try ephemeral.sharedSecretFromKeyAgreement(with: recipientPub)
        let symKey = deriveKey(shared, ephemeral.publicKey, recipientPub)
        let sealed = try ChaChaPoly.seal(Data(bytes: plainPtr, count: plainLen), using: symKey)
        setOut(ephemeral.publicKey.x963Representation + sealed.combined, outPtr, outLen)
        return statusOk
    } catch {
        return fail(statusFailure, "failed to wrap secret: \(error)", outPtr, outLen)
    }
}

@_cdecl("foundry_se_unwrap")
public func foundrySeUnwrap(
    _ blobPtr: UnsafePointer<UInt8>, _ blobLen: Int,
    _ sealedPtr: UnsafePointer<UInt8>, _ sealedLen: Int,
    _ policy: Int32,
    _ reasonPtr: UnsafePointer<CChar>?,
    _ outPtr: UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>,
    _ outLen: UnsafeMutablePointer<Int>
) -> Int32 {
    guard SecureEnclave.isAvailable else {
        return fail(
            statusUnavailable, "Secure Enclave is not available on this machine", outPtr, outLen)
    }
    guard sealedLen >= x963PublicKeyLen + chaChaPolyOverheadLen else {
        return fail(statusInvalidData, "sealed data is truncated", outPtr, outLen)
    }
    var reason = reasonPtr.map { String(cString: $0) } ?? "unlock the keystore"
    if reason.isEmpty { reason = "unlock the keystore" }
    let context = LAContext()
    // Also shown if the enclave ever drives its own prompt (e.g. re-evaluation).
    context.localizedReason = reason

    // Failures before any authentication are sidecar data problems.
    let key: SecureEnclave.P256.KeyAgreement.PrivateKey
    let ephemeralPub: P256.KeyAgreement.PublicKey
    let box: ChaChaPoly.SealedBox
    do {
        key = try SecureEnclave.P256.KeyAgreement.PrivateKey(
            dataRepresentation: Data(bytes: blobPtr, count: blobLen),
            authenticationContext: context)
        let sealed = Data(bytes: sealedPtr, count: sealedLen)
        ephemeralPub = try P256.KeyAgreement.PublicKey(
            x963Representation: sealed.prefix(x963PublicKeyLen))
        box = try ChaChaPoly.SealedBox(combined: sealed.dropFirst(x963PublicKeyLen))
    } catch {
        return fail(statusInvalidData, "invalid sidecar data: \(error)", outPtr, outLen)
    }

    do {
        try preauthenticate(context, policy: policy, reason: reason)
    } catch let error as ShimError {
        return fail(error.status, error.message, outPtr, outLen)
    } catch {
        return fail(statusFailure, "authentication failed: \(error)", outPtr, outLen)
    }

    let shared: SharedSecret
    do {
        // The enclave enforces the key's own access-control policy here; the
        // fresh authentication on `context` satisfies it without a second
        // prompt.
        shared = try key.sharedSecretFromKeyAgreement(with: ephemeralPub)
    } catch {
        // The user just authenticated successfully, so the enclave rejecting
        // the key means the enrollment no longer matches this device's state —
        // e.g. a `biometryCurrentSet` key after fingerprints changed.
        return fail(
            statusInvalidated, "Secure Enclave rejected the wrap key: \(error)", outPtr, outLen)
    }
    let symKey = deriveKey(shared, ephemeralPub, key.publicKey)
    do {
        setOut(try ChaChaPoly.open(box, using: symKey), outPtr, outLen)
        return statusOk
    } catch {
        // AEAD failure: the sealed password does not match this wrap key.
        return fail(
            statusInvalidData, "sealed password failed to authenticate: \(error)", outPtr, outLen)
    }
}

private func deriveKey(
    _ shared: SharedSecret, _ ephemeralPub: P256.KeyAgreement.PublicKey,
    _ recipientPub: P256.KeyAgreement.PublicKey
) -> SymmetricKey {
    shared.hkdfDerivedSymmetricKey(
        using: SHA256.self,
        salt: ephemeralPub.x963Representation + recipientPub.x963Representation,
        sharedInfo: hkdfInfo,
        outputByteCount: 32)
}

@_cdecl("foundry_se_free")
public func foundrySeFree(_ ptr: UnsafeMutablePointer<UInt8>?, _ len: Int) {
    if let ptr, len > 0 {
        // Buffers may hold the plaintext keystore password; scrub before freeing.
        memset_s(ptr, len, 0, len)
    }
    free(ptr)
}
