import {
  decodeBase64Url,
  decodeBase64UrlToString,
  encodeBase64Url,
  sha256HexBytes,
} from './auth'
import { getAccountInfo } from './entitlements'
import type { AppleVerifyRequest, Env } from './types'

const TEXT_ENCODER = new TextEncoder()
const TEXT_DECODER = new TextDecoder()

/**
 * Official Apple Root CA - G3 (DER, base64).
 * Subject/Issuer: CN=Apple Root CA - G3, OU=Apple Certification Authority, O=Apple Inc., C=US
 * Validity: 2014-04-30 to 2039-04-30, ECDSA P-384 (secp384r1)
 * SHA-256: 63343abfb89a6a03ebb57e9b3f5fa7be7c4f5c756f3017b3a8c488c3653e9179
 */
export const APPLE_ROOT_CA_G3_DER_B64 =
  'MIICQzCCAcmgAwIBAgIILcX8iNLFS5UwCgYIKoZIzj0EAwMwZzEbMBkGA1UEAwwSQXBwbGUgUm9vdCBDQSAtIEczMSYwJAYDVQQLDB1BcHBsZSBDZXJ0aWZpY2F0aW9uIEF1dGhvcml0eTETMBEGA1UECgwKQXBwbGUgSW5jLjELMAkGA1UEBhMCVVMwHhcNMTQwNDMwMTgxOTA2WhcNMzkwNDMwMTgxOTA2WjBnMRswGQYDVQQDDBJBcHBsZSBSb290IENBIC0gRzMxJjAkBgNVBAsMHUFwcGxlIENlcnRpZmljYXRpb24gQXV0aG9yaXR5MRMwEQYDVQQKDApBcHBsZSBJbmMuMQswCQYDVQQGEwJVUzB2MBAGByqGSM49AgEGBSuBBAAiA2IABJjpLz1AcqTtkyJygRMc3RCV8cWjTnHcFBbZDuWmBSp3ZHtfTjjTuxxEtX/1H7YyYl3J6YRbTzBPEVoA/VhYDKX1DyxNB0cTddqXl5dvMVztK517IDvYuVTZXpmkOlEKMaNCMEAwHQYDVR0OBBYEFLuw3qFYM4iapIqZ3r6966/ayySrMA8GA1UdEwEB/wQFMAMBAf8wDgYDVR0PAQH/BAQDAgEGMAoGCCqGSM49BAMDA2gAMGUCMQCD6cHEFl4aXTQY2e3v9GwOAEZLuN+yRhHFD/3meoyhpmvOwgPUnPWTxnS4at+qIxUCMG1mihDK1A3UT82NQz60imOlM27jbdoXt2QfyFMm+YhidDkLF1vLUagM6BgD56KyKA=='

const MAX_CHAIN_LENGTH = 5
const FUTURE_SIGNED_DATE_LEEWAY_MS = 5 * 60 * 1000
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
const BASE64URL_22_RE = /^[A-Za-z0-9_-]{22}$/

const OID_EC_PUBLIC_KEY = '1.2.840.10045.2.1'
const OID_BASIC_CONSTRAINTS = '2.5.29.19'

type EcCurve = 'P-256' | 'P-384' | 'P-521'
type EcHash = 'SHA-256' | 'SHA-384' | 'SHA-512'

const CURVE_BY_OID: Record<string, EcCurve> = {
  '1.2.840.10045.3.1.7': 'P-256',
  '1.3.132.0.34': 'P-384',
  '1.3.132.0.35': 'P-521',
}

const HASH_BY_SIG_OID: Record<string, EcHash> = {
  '1.2.840.10045.4.3.2': 'SHA-256',
  '1.2.840.10045.4.3.3': 'SHA-384',
  '1.2.840.10045.4.3.4': 'SHA-512',
}

interface DerNode {
  tag: number
  headerStart: number
  contentStart: number
  end: number
}

interface ParsedX509Cert {
  raw: Uint8Array
  tbsRaw: Uint8Array
  signatureAlgorithmOid: string
  signatureDerBytes: Uint8Array
  issuerRaw: Uint8Array
  subjectRaw: Uint8Array
  notBeforeMs: number
  notAfterMs: number
  spkiRaw: Uint8Array
  namedCurve: EcCurve
  isCa: boolean
}

export interface AppleTransactionPayload {
  transactionId?: string
  originalTransactionId?: string
  bundleId?: string
  productId?: string
  environment?: string
  purchaseDate?: number
  originalPurchaseDate?: number
  expiresDate?: number
  revocationDate?: number
  revocationReason?: number
  appAccountToken?: string
  signedDate?: number
  inAppOwnershipType?: string
}

export interface AppleRenewalInfoPayload {
  originalTransactionId?: string
  autoRenewProductId?: string
  productId?: string
  autoRenewStatus?: number
  expirationIntent?: number
  gracePeriodExpiresDate?: number
  isInBillingRetryPeriod?: boolean
  environment?: string
  signedDate?: number
}

export interface AppleNotificationPayload {
  notificationType?: string
  subtype?: string
  notificationUUID?: string
  version?: string
  signedDate?: number
  data?: {
    appAppleId?: number
    bundleId?: string
    environment?: string
    signedTransactionInfo?: string
    signedRenewalInfo?: string
    status?: number
  }
}

export interface AppleJwsVerifyOptions {
  trustedRootsDer?: Uint8Array[]
  nowMs?: number
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

function nonEmptyString(raw: unknown): string | null {
  if (typeof raw !== 'string') return null
  const trimmed = raw.trim()
  return trimmed.length > 0 ? trimmed : null
}

function finiteNumber(raw: unknown): number | null {
  return typeof raw === 'number' && Number.isFinite(raw) ? raw : null
}

export function decodeBase64Std(input: string): Uint8Array {
  const cleaned = input.replace(/\s+/g, '')
  const binary = atob(cleaned)
  const bytes = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i)
  }
  return bytes
}

function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.byteLength !== b.byteLength) return false
  let diff = 0
  for (let i = 0; i < a.byteLength; i++) {
    diff |= a[i] ^ b[i]
  }
  return diff === 0
}

function coordLenForCurve(curve: EcCurve): number {
  switch (curve) {
    case 'P-256':
      return 32
    case 'P-384':
      return 48
    case 'P-521':
      return 66
  }
}

// ─── Minimal Strict ASN.1 DER Parser ─────────────────────────────────────────

function readDerNode(bytes: Uint8Array, offset: number, limit: number): DerNode {
  if (offset + 2 > limit) {
    throw new Error('der_truncated_header')
  }
  const tag = bytes[offset]
  const lenByte = bytes[offset + 1]
  let contentStart = offset + 2
  let length = 0

  if ((lenByte & 0x80) === 0) {
    length = lenByte
  } else {
    const numOctets = lenByte & 0x7f
    if (numOctets === 0 || numOctets > 4) {
      throw new Error('der_invalid_length')
    }
    if (contentStart + numOctets > limit) {
      throw new Error('der_truncated_length')
    }
    for (let i = 0; i < numOctets; i++) {
      length = (length << 8) | bytes[contentStart + i]
    }
    contentStart += numOctets
  }

  const end = contentStart + length
  if (length < 0 || end > limit) {
    throw new Error('der_length_out_of_bounds')
  }

  return { tag, headerStart: offset, contentStart, end }
}

function readDerChildren(bytes: Uint8Array, container: DerNode): DerNode[] {
  const children: DerNode[] = []
  let pos = container.contentStart
  while (pos < container.end) {
    const node = readDerNode(bytes, pos, container.end)
    children.push(node)
    pos = node.end
  }
  if (pos !== container.end) {
    throw new Error('der_trailing_bytes')
  }
  return children
}

function decodeOid(bytes: Uint8Array, node: DerNode): string {
  if (node.tag !== 0x06 || node.contentStart >= node.end) {
    throw new Error('der_invalid_oid')
  }
  const first = bytes[node.contentStart]
  const arcs: number[] = [Math.floor(first / 40), first % 40]
  let value = 0
  let inArc = false
  for (let i = node.contentStart + 1; i < node.end; i++) {
    const b = bytes[i]
    value = value * 128 + (b & 0x7f)
    inArc = true
    if ((b & 0x80) === 0) {
      arcs.push(value)
      value = 0
      inArc = false
    }
  }
  if (inArc) {
    throw new Error('der_truncated_oid')
  }
  return arcs.join('.')
}

function parseAsn1TimeMs(bytes: Uint8Array, node: DerNode): number {
  const str = TEXT_DECODER.decode(bytes.subarray(node.contentStart, node.end))
  if (node.tag === 0x17) {
    // UTCTime: YYMMDDHHMMSSZ
    const match = /^(\d{2})(\d{2})(\d{2})(\d{2})(\d{2})(\d{2})Z$/.exec(str)
    if (!match) throw new Error('der_invalid_utc_time')
    const yy = Number.parseInt(match[1], 10)
    const year = yy >= 50 ? 1900 + yy : 2000 + yy
    return Date.UTC(
      year,
      Number.parseInt(match[2], 10) - 1,
      Number.parseInt(match[3], 10),
      Number.parseInt(match[4], 10),
      Number.parseInt(match[5], 10),
      Number.parseInt(match[6], 10),
    )
  }
  if (node.tag === 0x18) {
    // GeneralizedTime: YYYYMMDDHHMMSS(.fff)?Z
    const match = /^(\d{4})(\d{2})(\d{2})(\d{2})(\d{2})(\d{2})(?:\.\d+)?Z$/.exec(str)
    if (!match) throw new Error('der_invalid_generalized_time')
    return Date.UTC(
      Number.parseInt(match[1], 10),
      Number.parseInt(match[2], 10) - 1,
      Number.parseInt(match[3], 10),
      Number.parseInt(match[4], 10),
      Number.parseInt(match[5], 10),
      Number.parseInt(match[6], 10),
    )
  }
  throw new Error('der_unsupported_time_tag')
}

function parseBasicConstraintsIsCa(extnValueBytes: Uint8Array): boolean {
  const seq = readDerNode(extnValueBytes, 0, extnValueBytes.byteLength)
  if (seq.tag !== 0x30) return false
  const children = readDerChildren(extnValueBytes, seq)
  if (children.length === 0) return false
  const first = children[0]
  if (first.tag === 0x01 && first.end - first.contentStart === 1) {
    return extnValueBytes[first.contentStart] !== 0x00
  }
  return false
}

function parseX509Certificate(der: Uint8Array): ParsedX509Cert {
  const certSeq = readDerNode(der, 0, der.byteLength)
  if (certSeq.tag !== 0x30 || certSeq.end !== der.byteLength) {
    throw new Error('x509_invalid_outer_sequence')
  }
  const certFields = readDerChildren(der, certSeq)
  if (certFields.length !== 3) {
    throw new Error('x509_invalid_field_count')
  }

  const [tbsNode, sigAlgNode, sigValNode] = certFields
  if (tbsNode.tag !== 0x30 || sigAlgNode.tag !== 0x30 || sigValNode.tag !== 0x03) {
    throw new Error('x509_invalid_top_level_tags')
  }

  const sigAlgChildren = readDerChildren(der, sigAlgNode)
  if (sigAlgChildren.length < 1) {
    throw new Error('x509_missing_sig_alg_oid')
  }
  const signatureAlgorithmOid = decodeOid(der, sigAlgChildren[0])

  if (sigValNode.end - sigValNode.contentStart < 2 || der[sigValNode.contentStart] !== 0x00) {
    throw new Error('x509_invalid_signature_bitstring')
  }
  const signatureDerBytes = der.subarray(sigValNode.contentStart + 1, sigValNode.end)

  const tbsChildren = readDerChildren(der, tbsNode)
  let idx = 0
  if (idx < tbsChildren.length && tbsChildren[idx].tag === 0xa0) {
    idx += 1 // optional explicit [0] version
  }
  // serialNumber, signature, issuer, validity, subject, subjectPublicKeyInfo
  if (idx + 6 > tbsChildren.length) {
    throw new Error('x509_tbs_too_short')
  }
  const issuerNode = tbsChildren[idx + 2]
  const validityNode = tbsChildren[idx + 3]
  const subjectNode = tbsChildren[idx + 4]
  const spkiNode = tbsChildren[idx + 5]

  if (
    issuerNode.tag !== 0x30 ||
    validityNode.tag !== 0x30 ||
    subjectNode.tag !== 0x30 ||
    spkiNode.tag !== 0x30
  ) {
    throw new Error('x509_invalid_tbs_structure')
  }

  const validityChildren = readDerChildren(der, validityNode)
  if (validityChildren.length !== 2) {
    throw new Error('x509_invalid_validity')
  }
  const notBeforeMs = parseAsn1TimeMs(der, validityChildren[0])
  const notAfterMs = parseAsn1TimeMs(der, validityChildren[1])

  const spkiChildren = readDerChildren(der, spkiNode)
  if (spkiChildren.length < 2 || spkiChildren[0].tag !== 0x30) {
    throw new Error('x509_invalid_spki')
  }
  const spkiAlgChildren = readDerChildren(der, spkiChildren[0])
  if (spkiAlgChildren.length < 2) {
    throw new Error('x509_unsupported_key_algorithm')
  }
  const keyAlgOid = decodeOid(der, spkiAlgChildren[0])
  if (keyAlgOid !== OID_EC_PUBLIC_KEY) {
    throw new Error('x509_non_ec_public_key')
  }
  const curveOid = decodeOid(der, spkiAlgChildren[1])
  const namedCurve = CURVE_BY_OID[curveOid]
  if (!namedCurve) {
    throw new Error(`x509_unsupported_curve:${curveOid}`)
  }

  let isCa = false
  for (let i = idx + 6; i < tbsChildren.length; i++) {
    const extWrapper = tbsChildren[i]
    if (extWrapper.tag !== 0xa3) continue
    const extSeqList = readDerChildren(der, extWrapper)
    if (extSeqList.length !== 1 || extSeqList[0].tag !== 0x30) continue
    const extensions = readDerChildren(der, extSeqList[0])
    for (const ext of extensions) {
      if (ext.tag !== 0x30) continue
      const extFields = readDerChildren(der, ext)
      if (extFields.length < 2) continue
      const extOid = decodeOid(der, extFields[0])
      if (extOid === OID_BASIC_CONSTRAINTS) {
        const valueNode = extFields[extFields.length - 1]
        if (valueNode.tag === 0x04) {
          isCa = parseBasicConstraintsIsCa(der.subarray(valueNode.contentStart, valueNode.end))
        }
      }
    }
  }

  return {
    raw: der,
    tbsRaw: der.subarray(tbsNode.headerStart, tbsNode.end),
    signatureAlgorithmOid,
    signatureDerBytes,
    issuerRaw: der.subarray(issuerNode.headerStart, issuerNode.end),
    subjectRaw: der.subarray(subjectNode.headerStart, subjectNode.end),
    notBeforeMs,
    notAfterMs,
    spkiRaw: der.subarray(spkiNode.headerStart, spkiNode.end),
    namedCurve,
    isCa,
  }
}

function derEcdsaSignatureToP1363(derSig: Uint8Array, coordLen: number): Uint8Array {
  const seq = readDerNode(derSig, 0, derSig.byteLength)
  if (seq.tag !== 0x30 || seq.end !== derSig.byteLength) {
    throw new Error('ecdsa_invalid_der_sig')
  }
  const parts = readDerChildren(derSig, seq)
  if (parts.length !== 2 || parts[0].tag !== 0x02 || parts[1].tag !== 0x02) {
    throw new Error('ecdsa_invalid_der_integers')
  }

  const out = new Uint8Array(coordLen * 2)
  for (let i = 0; i < 2; i++) {
    let start = parts[i].contentStart
    const end = parts[i].end
    while (start < end - 1 && derSig[start] === 0x00) {
      start += 1
    }
    const len = end - start
    if (len <= 0 || len > coordLen) {
      throw new Error('ecdsa_integer_out_of_range')
    }
    out.set(derSig.subarray(start, end), i * coordLen + (coordLen - len))
  }
  return out
}

async function verifyCertSignedBy(child: ParsedX509Cert, parent: ParsedX509Cert): Promise<boolean> {
  if (!parent.isCa) return false
  if (!bytesEqual(child.issuerRaw, parent.subjectRaw)) return false

  const hash = HASH_BY_SIG_OID[child.signatureAlgorithmOid]
  if (!hash) return false

  const parentKey = await crypto.subtle.importKey(
    'spki',
    parent.spkiRaw,
    { name: 'ECDSA', namedCurve: parent.namedCurve },
    false,
    ['verify'],
  )
  const p1363Sig = derEcdsaSignatureToP1363(
    child.signatureDerBytes,
    coordLenForCurve(parent.namedCurve),
  )
  return crypto.subtle.verify(
    { name: 'ECDSA', hash },
    parentKey,
    p1363Sig,
    child.tbsRaw,
  )
}

function getTrustedRootDers(env?: Env, override?: Uint8Array[]): Uint8Array[] {
  if (override && override.length > 0) {
    return override
  }
  const envRoot = env?.APPLE_ROOT_CA_DER_B64?.trim()
  if (envRoot && envRoot.length > 0) {
    return [decodeBase64Std(envRoot)]
  }
  return [decodeBase64Std(APPLE_ROOT_CA_G3_DER_B64)]
}

/**
 * Cryptographically verify an Apple StoreKit 2 / App Store Server Notifications v2 JWS
 * (ES256 signature + full `x5c` X.509 certificate chain anchored to pinned Apple Root CA - G3).
 */
export async function verifyAppleJws<T extends { signedDate?: number }>(
  jws: string,
  env?: Env,
  options?: AppleJwsVerifyOptions,
): Promise<{ ok: true; payload: T } | { ok: false; error: string }> {
  const parts = jws.trim().split('.')
  if (parts.length !== 3) {
    return { ok: false, error: 'invalid_jws_format' }
  }
  const [headerB64, payloadB64, signatureB64] = parts

  let header: { alg?: unknown; x5c?: unknown }
  let payload: T
  let signatureBytes: Uint8Array
  try {
    header = JSON.parse(decodeBase64UrlToString(headerB64)) as { alg?: unknown; x5c?: unknown }
    payload = JSON.parse(decodeBase64UrlToString(payloadB64)) as T
    signatureBytes = decodeBase64Url(signatureB64)
  } catch {
    return { ok: false, error: 'invalid_jws_encoding' }
  }

  if (header.alg !== 'ES256') {
    return { ok: false, error: 'unsupported_jws_alg' }
  }
  if (signatureBytes.byteLength !== 64) {
    return { ok: false, error: 'invalid_jws_signature_length' }
  }
  if (
    !Array.isArray(header.x5c) ||
    header.x5c.length < 2 ||
    header.x5c.length > MAX_CHAIN_LENGTH ||
    !header.x5c.every((entry) => typeof entry === 'string' && entry.length > 0)
  ) {
    return { ok: false, error: 'invalid_x5c_chain' }
  }

  let chain: ParsedX509Cert[]
  try {
    chain = (header.x5c as string[]).map((b64) => parseX509Certificate(decodeBase64Std(b64)))
  } catch {
    return { ok: false, error: 'invalid_x5c_certificate' }
  }

  let trustedRoots: ParsedX509Cert[]
  try {
    trustedRoots = getTrustedRootDers(env, options?.trustedRootsDer).map((der) =>
      parseX509Certificate(der),
    )
  } catch {
    return { ok: false, error: 'invalid_trusted_root_config' }
  }

  const nowMs = options?.nowMs ?? Date.now()
  const signedDate = finiteNumber(payload?.signedDate)
  if (signedDate !== null && signedDate > nowMs + FUTURE_SIGNED_DATE_LEEWAY_MS) {
    return { ok: false, error: 'jws_signed_date_in_future' }
  }
  const effectiveTimeMs = signedDate ?? nowMs

  // Verify every certificate link in x5c (leaf -> intermediate -> ...).
  for (let i = 0; i < chain.length - 1; i++) {
    const child = chain[i]
    const parent = chain[i + 1]
    try {
      const ok = await verifyCertSignedBy(child, parent)
      if (!ok) {
        return { ok: false, error: 'x5c_chain_signature_invalid' }
      }
    } catch {
      return { ok: false, error: 'x5c_chain_signature_invalid' }
    }
  }

  // Anchor the top of the chain to a pinned trusted root CA.
  const topCert = chain[chain.length - 1]
  const topHash = await sha256HexBytes(topCert.raw)
  let matchedRoot: ParsedX509Cert | null = null

  for (const root of trustedRoots) {
    const rootHash = await sha256HexBytes(root.raw)
    if (topHash === rootHash) {
      matchedRoot = root
      break
    }
  }

  if (!matchedRoot) {
    // Allow 2-cert chains where Apple omits the self-signed root and sends [leaf, intermediate].
    for (const root of trustedRoots) {
      try {
        if (await verifyCertSignedBy(topCert, root)) {
          matchedRoot = root
          break
        }
      } catch {
        // Try next trusted root
      }
    }
  }

  if (!matchedRoot) {
    return { ok: false, error: 'x5c_untrusted_root' }
  }

  // Verify validity timestamps across the entire chain + matched root.
  const certsToCheck = [...chain, matchedRoot]
  for (const cert of certsToCheck) {
    if (effectiveTimeMs < cert.notBeforeMs || effectiveTimeMs > cert.notAfterMs) {
      return { ok: false, error: 'x5c_certificate_expired' }
    }
  }

  // Verify JWS ES256 signature with the leaf certificate's P-256 public key.
  const leaf = chain[0]
  if (leaf.namedCurve !== 'P-256') {
    return { ok: false, error: 'x5c_leaf_not_p256' }
  }

  try {
    const leafKey = await crypto.subtle.importKey(
      'spki',
      leaf.spkiRaw,
      { name: 'ECDSA', namedCurve: 'P-256' },
      false,
      ['verify'],
    )
    const signingInput = TEXT_ENCODER.encode(`${headerB64}.${payloadB64}`)
    const valid = await crypto.subtle.verify(
      { name: 'ECDSA', hash: 'SHA-256' },
      leafKey,
      signatureBytes,
      signingInput,
    )
    if (!valid) {
      return { ok: false, error: 'invalid_jws_signature' }
    }
  } catch {
    return { ok: false, error: 'invalid_jws_signature' }
  }

  return { ok: true, payload }
}

// ─── Deterministic appAccountToken Mapping (Mirrored in iOS StoreKitManager) ─

function formatUuidBytes(bytes: Uint8Array): string {
  let hex = ''
  for (let i = 0; i < 16; i++) {
    hex += bytes[i].toString(16).padStart(2, '0')
  }
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20, 32)}`
}

function parseUuidBytes(uuid: string): Uint8Array | null {
  if (!UUID_RE.test(uuid)) return null
  const hex = uuid.replace(/-/g, '')
  const bytes = new Uint8Array(16)
  for (let i = 0; i < 16; i++) {
    bytes[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16)
  }
  return bytes
}

/**
 * Deterministic UUID derived from `userId` (mirrored identically in Swift `StoreKitManager.appAccountToken`).
 * - For standard `usr_<22-char-base64url>` IDs (16 random bytes): encodes those exact 16 bytes so ASN v2
 *   webhooks can reverse the token back to `userId` in O(1) even before `/verify` has been called.
 * - For canonical UUID user IDs: returns the lowercased UUID directly.
 * - For all other IDs: returns an RFC 9562 UUIDv8 derived from SHA-256("rem:apple-account-token:v1:" + userId).
 */
export async function deriveAppleAppAccountToken(userId: string): Promise<string> {
  if (userId.startsWith('usr_')) {
    const suffix = userId.slice(4)
    if (BASE64URL_22_RE.test(suffix)) {
      try {
        const raw = decodeBase64Url(suffix)
        if (raw.byteLength === 16) {
          return formatUuidBytes(raw)
        }
      } catch {
        // Fall through to hashed UUIDv8
      }
    }
  }

  if (UUID_RE.test(userId)) {
    return userId.toLowerCase()
  }

  const digest = new Uint8Array(
    await crypto.subtle.digest('SHA-256', TEXT_ENCODER.encode(`rem:apple-account-token:v1:${userId}`)),
  )
  const uuidBytes = digest.slice(0, 16)
  uuidBytes[6] = (uuidBytes[6] & 0x0f) | 0x80 // UUID v8
  uuidBytes[8] = (uuidBytes[8] & 0x3f) | 0x80 // RFC 4122/9562 variant
  return formatUuidBytes(uuidBytes)
}

export function userIdFromReversibleAppAccountToken(token: string): string | null {
  const bytes = parseUuidBytes(token)
  if (!bytes) return null
  return `usr_${encodeBase64Url(bytes)}`
}

// ─── Configuration Helpers ───────────────────────────────────────────────────

export function isAppleBillingConfigured(env: Env): boolean {
  return Boolean(env.APPLE_BUNDLE_ID?.trim() && env.APPLE_PRODUCT_ID?.trim())
}

function getAllowedAppleEnvironments(env: Env): Set<string> {
  const raw = env.APPLE_ENVIRONMENT?.trim()
  if (!raw) {
    // Allow both Production and Sandbox (TestFlight / App Review) unless explicitly pinned.
    return new Set(['Production', 'Sandbox'])
  }
  const parts = raw
    .split(',')
    .map((p) => p.trim())
    .filter((p) => p.length > 0)
  return new Set(parts.length > 0 ? parts : ['Production', 'Sandbox'])
}

function isEnvironmentAllowed(env: Env, actual: string | null): boolean {
  if (!actual) return false
  return getAllowedAppleEnvironments(env).has(actual)
}

// ─── POST /v1/billing/apple/verify ───────────────────────────────────────────

export async function handleVerifyAppleTransaction(
  request: Request,
  env: Env,
  userId: string,
  authMethod: 'jwt' | 'dev_token',
  nowMs = Date.now(),
): Promise<Response> {
  if (authMethod === 'dev_token' || !env.DB) {
    return jsonResponse(
      {
        error: 'billing_unavailable',
        message: 'App Store verification requires a signed-in rem Premium account.',
      },
      400,
    )
  }

  if (!isAppleBillingConfigured(env)) {
    return jsonResponse(
      {
        error: 'apple_billing_not_configured',
        message: 'App Store billing is not configured on this server yet.',
      },
      503,
    )
  }

  let body: AppleVerifyRequest
  try {
    body = (await request.json()) as AppleVerifyRequest
  } catch {
    return jsonResponse({ error: 'invalid_json', message: 'Invalid request body.' }, 400)
  }

  const signedTransactionInfo = nonEmptyString(body?.signedTransactionInfo)
  if (!signedTransactionInfo) {
    return jsonResponse(
      { error: 'missing_signed_transaction', message: 'Missing signedTransactionInfo.' },
      400,
    )
  }

  const verified = await verifyAppleJws<AppleTransactionPayload>(signedTransactionInfo, env, {
    nowMs,
  })
  if (!verified.ok) {
    return jsonResponse(
      { error: verified.error, message: 'Could not verify App Store transaction signature.' },
      400,
    )
  }

  const tx = verified.payload
  const expectedBundleId = env.APPLE_BUNDLE_ID!.trim()
  const expectedProductId = env.APPLE_PRODUCT_ID!.trim()

  if (nonEmptyString(tx.bundleId) !== expectedBundleId) {
    return jsonResponse(
      { error: 'bundle_id_mismatch', message: 'App Store transaction bundle ID does not match.' },
      400,
    )
  }

  if (nonEmptyString(tx.productId) !== expectedProductId) {
    return jsonResponse(
      { error: 'product_id_mismatch', message: 'App Store transaction product ID does not match.' },
      400,
    )
  }

  const txEnv = nonEmptyString(tx.environment)
  if (!isEnvironmentAllowed(env, txEnv)) {
    return jsonResponse(
      { error: 'environment_mismatch', message: 'App Store transaction environment is not allowed.' },
      400,
    )
  }

  const originalTransactionId = nonEmptyString(tx.originalTransactionId)
  if (!originalTransactionId) {
    return jsonResponse(
      { error: 'missing_original_transaction_id', message: 'Missing originalTransactionId.' },
      400,
    )
  }

  // Prevent sharing a single App Store subscription across multiple rem accounts.
  const existingOwner = await env.DB.prepare(
    'SELECT user_id FROM user_entitlements WHERE original_transaction_id = ?1',
  )
    .bind(originalTransactionId)
    .first<{ user_id: string }>()

  if (existingOwner && existingOwner.user_id !== userId) {
    return jsonResponse(
      {
        error: 'apple_transaction_already_linked',
        message: 'This App Store subscription is already linked to another rem account.',
      },
      409,
    )
  }

  const revocationDate = finiteNumber(tx.revocationDate)
  const expiresDate = finiteNumber(tx.expiresDate)
  const appAccountToken =
    nonEmptyString(tx.appAccountToken)?.toLowerCase() ?? (await deriveAppleAppAccountToken(userId))

  const userExisting = await env.DB.prepare(
    'SELECT provider, status, original_transaction_id FROM user_entitlements WHERE user_id = ?1',
  )
    .bind(userId)
    .first<{
      provider: string | null
      status: string
      original_transaction_id: string | null
    }>()

  const sameOriginalTx = userExisting?.original_transaction_id === originalTransactionId

  if (revocationDate !== null) {
    if (sameOriginalTx) {
      await env.DB.prepare(
        `UPDATE user_entitlements
         SET status = 'canceled',
             current_period_end = ?2,
             cancel_at_period_end = 1,
             updated_at = ?3
         WHERE user_id = ?1`,
      )
        .bind(userId, Math.min(revocationDate, nowMs), nowMs)
        .run()
      const info = await getAccountInfo(env, userId, authMethod, nowMs)
      return jsonResponse(info)
    }
    return jsonResponse(
      {
        error: 'transaction_revoked',
        message: 'This App Store transaction has been refunded or revoked.',
      },
      400,
    )
  }

  if (expiresDate !== null && expiresDate <= nowMs) {
    if (sameOriginalTx) {
      await env.DB.prepare(
        `UPDATE user_entitlements
         SET status = 'canceled',
             current_period_end = ?2,
             cancel_at_period_end = 1,
             updated_at = ?3
         WHERE user_id = ?1`,
      )
        .bind(userId, expiresDate, nowMs)
        .run()
      const info = await getAccountInfo(env, userId, authMethod, nowMs)
      return jsonResponse(info)
    }
    return jsonResponse(
      {
        error: 'transaction_expired',
        message: 'This App Store subscription has expired.',
      },
      400,
    )
  }

  await env.DB.prepare(
    `INSERT INTO user_entitlements (
       user_id, status, plan, provider, provider_customer_id, provider_subscription_id,
       current_period_end, cancel_at_period_end, trial_started_at, original_transaction_id, updated_at
     ) VALUES (?1, 'active', 'premium_yearly', 'apple', ?2, ?3, ?4, 0, NULL, ?3, ?5)
     ON CONFLICT(user_id) DO UPDATE SET
       status = 'active',
       plan = excluded.plan,
       provider = 'apple',
       provider_customer_id = COALESCE(excluded.provider_customer_id, user_entitlements.provider_customer_id),
       provider_subscription_id = excluded.provider_subscription_id,
       original_transaction_id = excluded.original_transaction_id,
       current_period_end = excluded.current_period_end,
       cancel_at_period_end = CASE
         WHEN user_entitlements.original_transaction_id = excluded.original_transaction_id
              AND user_entitlements.current_period_end = excluded.current_period_end
         THEN user_entitlements.cancel_at_period_end
         ELSE 0
       END,
       updated_at = excluded.updated_at`,
  )
    .bind(userId, appAccountToken, originalTransactionId, expiresDate, nowMs)
    .run()

  const updated = await getAccountInfo(env, userId, authMethod, nowMs)
  return jsonResponse(updated)
}

// ─── POST /v1/webhooks/apple (App Store Server Notifications v2) ──────────────

async function resolveAppleWebhookUserId(
  db: D1Database,
  originalTransactionId: string | null,
  appAccountToken: string | null,
): Promise<string | null> {
  if (originalTransactionId) {
    const byTx = await db
      .prepare('SELECT user_id FROM user_entitlements WHERE original_transaction_id = ?1')
      .bind(originalTransactionId)
      .first<{ user_id: string }>()
    if (byTx) return byTx.user_id
  }

  if (appAccountToken) {
    const normalized = appAccountToken.toLowerCase()
    const byCustomer = await db
      .prepare(
        `SELECT user_id FROM user_entitlements
         WHERE provider = 'apple' AND provider_customer_id = ?1`,
      )
      .bind(normalized)
      .first<{ user_id: string }>()
    if (byCustomer) return byCustomer.user_id

    const reversedUserId = userIdFromReversibleAppAccountToken(normalized)
    if (reversedUserId) {
      const user = await db
        .prepare('SELECT id FROM users WHERE id = ?1')
        .bind(reversedUserId)
        .first<{ id: string }>()
      if (user) return user.id
    }

    const directUser = await db
      .prepare('SELECT id FROM users WHERE id = ?1')
      .bind(normalized)
      .first<{ id: string }>()
    if (directUser) return directUser.id
  }

  return null
}

const HANDLED_APPLE_NOTIFICATIONS = new Set([
  'SUBSCRIBED',
  'DID_RENEW',
  'OFFER_REDEEMED',
  'DID_CHANGE_RENEWAL_STATUS',
  'DID_FAIL_TO_RENEW',
  'GRACE_PERIOD_EXPIRED',
  'EXPIRED',
  'REFUND',
  'REVOKE',
  'REFUND_REVERSED',
])

export async function handleAppleWebhook(
  request: Request,
  env: Env,
  nowMs = Date.now(),
): Promise<Response> {
  if (!env.DB || !isAppleBillingConfigured(env)) {
    return jsonResponse({ error: 'apple_webhook_not_configured' }, 503)
  }

  let body: { signedPayload?: unknown }
  try {
    body = (await request.json()) as { signedPayload?: unknown }
  } catch {
    return jsonResponse({ error: 'invalid_json' }, 400)
  }

  const signedPayload = nonEmptyString(body?.signedPayload)
  if (!signedPayload) {
    return jsonResponse({ error: 'missing_signed_payload' }, 400)
  }

  const outerVerified = await verifyAppleJws<AppleNotificationPayload>(signedPayload, env, {
    nowMs,
  })
  if (!outerVerified.ok) {
    return jsonResponse({ error: outerVerified.error }, 401)
  }

  const notification = outerVerified.payload
  const notificationType = nonEmptyString(notification.notificationType)
  const notificationUUID = nonEmptyString(notification.notificationUUID)
  const subtype = nonEmptyString(notification.subtype)

  if (!notificationType || !notificationUUID) {
    return jsonResponse({ error: 'invalid_notification_payload' }, 400)
  }

  // Deduplicate notifications idempotently before applying state transitions.
  const inserted = await env.DB.prepare(
    `INSERT OR IGNORE INTO billing_events(provider, event_id, processed_at)
     VALUES ('apple', ?1, ?2)`,
  )
    .bind(notificationUUID, nowMs)
    .run()

  if ((inserted.meta.changes ?? 0) === 0) {
    return jsonResponse({ received: true, duplicate: true })
  }

  if (notificationType === 'TEST') {
    return jsonResponse({ received: true, test: true })
  }

  if (!HANDLED_APPLE_NOTIFICATIONS.has(notificationType)) {
    return jsonResponse({ received: true, ignored: true, reason: 'unhandled_notification_type' })
  }

  const expectedBundleId = env.APPLE_BUNDLE_ID!.trim()
  const expectedProductId = env.APPLE_PRODUCT_ID!.trim()

  const data = notification.data
  if (!data || typeof data !== 'object') {
    return jsonResponse({ error: 'missing_notification_data' }, 400)
  }

  const dataBundleId = nonEmptyString(data.bundleId)
  if (dataBundleId && dataBundleId !== expectedBundleId) {
    return jsonResponse({ received: true, ignored: true, reason: 'bundle_id_mismatch' })
  }

  const dataEnv = nonEmptyString(data.environment)
  if (dataEnv && !isEnvironmentAllowed(env, dataEnv)) {
    return jsonResponse({ received: true, ignored: true, reason: 'environment_mismatch' })
  }

  const signedTransactionInfo = nonEmptyString(data.signedTransactionInfo)
  if (!signedTransactionInfo) {
    return jsonResponse({ error: 'missing_signed_transaction_info' }, 400)
  }

  const txVerified = await verifyAppleJws<AppleTransactionPayload>(signedTransactionInfo, env, {
    nowMs,
  })
  if (!txVerified.ok) {
    return jsonResponse({ error: txVerified.error }, 401)
  }

  const tx = txVerified.payload
  if (nonEmptyString(tx.bundleId) !== expectedBundleId) {
    return jsonResponse({ received: true, ignored: true, reason: 'bundle_id_mismatch' })
  }
  if (nonEmptyString(tx.productId) !== expectedProductId) {
    return jsonResponse({ received: true, ignored: true, reason: 'product_id_mismatch' })
  }
  const txEnv = nonEmptyString(tx.environment) ?? dataEnv
  if (!isEnvironmentAllowed(env, txEnv)) {
    return jsonResponse({ received: true, ignored: true, reason: 'environment_mismatch' })
  }

  let renewal: AppleRenewalInfoPayload | null = null
  const signedRenewalInfo = nonEmptyString(data.signedRenewalInfo)
  if (signedRenewalInfo) {
    const renewalVerified = await verifyAppleJws<AppleRenewalInfoPayload>(signedRenewalInfo, env, {
      nowMs,
    })
    if (!renewalVerified.ok) {
      return jsonResponse({ error: renewalVerified.error }, 401)
    }
    renewal = renewalVerified.payload
  }

  const originalTransactionId = nonEmptyString(tx.originalTransactionId)
  const appAccountToken = nonEmptyString(tx.appAccountToken)?.toLowerCase() ?? null

  const userId = await resolveAppleWebhookUserId(env.DB, originalTransactionId, appAccountToken)
  if (!userId) {
    return jsonResponse({ received: true, ignored: true, reason: 'user_not_found' })
  }

  const expiresDate = finiteNumber(tx.expiresDate)
  const revocationDate = finiteNumber(tx.revocationDate)
  const gracePeriodExpiresDate = finiteNumber(renewal?.gracePeriodExpiresDate)
  const autoRenewStatus = finiteNumber(renewal?.autoRenewStatus)

  let mappedStatus: 'active' | 'past_due' | 'canceled'
  let periodEnd: number | null = expiresDate
  let cancelAtPeriodEnd = autoRenewStatus === 0

  switch (notificationType) {
    case 'SUBSCRIBED':
    case 'DID_RENEW':
    case 'OFFER_REDEEMED': {
      if (revocationDate !== null) {
        mappedStatus = 'canceled'
        cancelAtPeriodEnd = true
        periodEnd = Math.min(revocationDate, nowMs)
      } else {
        mappedStatus = 'active'
      }
      break
    }

    case 'DID_CHANGE_RENEWAL_STATUS': {
      if (subtype === 'AUTO_RENEW_DISABLED' || autoRenewStatus === 0) {
        cancelAtPeriodEnd = true
      } else if (subtype === 'AUTO_RENEW_ENABLED' || autoRenewStatus === 1) {
        cancelAtPeriodEnd = false
      }
      const stillValid = revocationDate === null && (expiresDate === null || expiresDate > nowMs)
      mappedStatus = stillValid ? 'active' : 'canceled'
      break
    }

    case 'DID_FAIL_TO_RENEW': {
      mappedStatus = 'past_due'
      if (subtype === 'GRACE_PERIOD') {
        periodEnd = gracePeriodExpiresDate ?? expiresDate ?? nowMs
      } else {
        // Billing retry without active grace period -> pause sync immediately.
        periodEnd = expiresDate !== null ? Math.min(expiresDate, nowMs) : nowMs
      }
      break
    }

    case 'GRACE_PERIOD_EXPIRED': {
      mappedStatus = 'past_due'
      const graceEnd = gracePeriodExpiresDate ?? expiresDate
      periodEnd = graceEnd !== null ? Math.min(graceEnd, nowMs) : nowMs
      break
    }

    case 'EXPIRED': {
      mappedStatus = 'canceled'
      cancelAtPeriodEnd = true
      periodEnd = expiresDate !== null ? Math.min(expiresDate, nowMs) : nowMs
      break
    }

    case 'REFUND':
    case 'REVOKE': {
      mappedStatus = 'canceled'
      cancelAtPeriodEnd = true
      periodEnd = revocationDate !== null ? Math.min(revocationDate, nowMs) : nowMs
      break
    }

    case 'REFUND_REVERSED': {
      const stillValid = revocationDate === null && (expiresDate === null || expiresDate > nowMs)
      mappedStatus = stillValid ? 'active' : 'canceled'
      break
    }

    default:
      return jsonResponse({ received: true, ignored: true })
  }

  // Never overwrite an active Polar subscription with a stale Apple cancellation/expiration.
  if (mappedStatus === 'canceled') {
    const existing = await env.DB.prepare(
      'SELECT provider, status FROM user_entitlements WHERE user_id = ?1',
    )
      .bind(userId)
      .first<{ provider: string | null; status: string }>()
    if (existing?.provider === 'polar' && existing.status === 'active') {
      return jsonResponse({ received: true, ignored: true, reason: 'active_polar_subscription' })
    }
  }

  const storedCustomerToken = appAccountToken ?? (await deriveAppleAppAccountToken(userId))

  await env.DB.prepare(
    `INSERT INTO user_entitlements (
       user_id, status, plan, provider, provider_customer_id, provider_subscription_id,
       current_period_end, cancel_at_period_end, trial_started_at, original_transaction_id, updated_at
     ) VALUES (?1, ?2, 'premium_yearly', 'apple', ?3, ?4, ?5, ?6, NULL, ?4, ?7)
     ON CONFLICT(user_id) DO UPDATE SET
       status = excluded.status,
       plan = excluded.plan,
       provider = 'apple',
       provider_customer_id = COALESCE(excluded.provider_customer_id, user_entitlements.provider_customer_id),
       provider_subscription_id = COALESCE(excluded.provider_subscription_id, user_entitlements.provider_subscription_id),
       original_transaction_id = COALESCE(excluded.original_transaction_id, user_entitlements.original_transaction_id),
       current_period_end = excluded.current_period_end,
       cancel_at_period_end = excluded.cancel_at_period_end,
       updated_at = excluded.updated_at`,
  )
    .bind(
      userId,
      mappedStatus,
      storedCustomerToken,
      originalTransactionId,
      periodEnd,
      cancelAtPeriodEnd ? 1 : 0,
      nowMs,
    )
    .run()

  return jsonResponse({ received: true })
}
