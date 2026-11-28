import type { Env } from './types'

const USER_ID_RE = /^[a-zA-Z0-9_-]{1,64}$/
const TEXT_ENCODER = new TextEncoder()
const TEXT_DECODER = new TextDecoder()

export interface AuthContext {
  userId: string
  authMethod: 'jwt' | 'dev_token'
}

export function isValidUserId(userId: string): boolean {
  return USER_ID_RE.test(userId)
}

export async function authenticateRequest(
  request: Request,
  env: Env,
): Promise<AuthContext | null> {
  const header = request.headers.get('Authorization')
  if (!header) return null
  const match = /^Bearer\s+(\S+)$/i.exec(header.trim())
  if (!match) return null
  const token = match[1]

  if (env.JWT_SECRET && token.split('.').length === 3) {
    const payload = await verifyHs256Jwt(token, env.JWT_SECRET)
    if (
      payload &&
      typeof payload.sub === 'string' &&
      isValidUserId(payload.sub) &&
      (payload.typ === undefined || payload.typ === 'access')
    ) {
      return { userId: payload.sub, authMethod: 'jwt' }
    }
  }

  if (env.DEV_AUTH_TOKEN && env.DEV_AUTH_TOKEN.length > 0) {
    const userId = verifyDevToken(token, env.DEV_AUTH_TOKEN)
    if (userId) return { userId, authMethod: 'dev_token' }
  }

  return null
}

export async function verifyJwtAccountExists(env: Env, userId: string): Promise<boolean> {
  if (!env.DB) return true
  try {
    const row = await env.DB.prepare('SELECT id FROM users WHERE id = ?1')
      .bind(userId)
      .first<{ id: string }>()
    return row !== null
  } catch (err) {
    const message = err instanceof Error ? err.message : ''
    if (message.includes('no such table')) {
      return true
    }
    throw err
  }
}

function verifyDevToken(token: string, expectedSecret: string): string | null {
  const colonIdx = token.indexOf(':')
  if (colonIdx === -1) {
    return timingSafeStringEqual(token, expectedSecret) ? 'dev-user' : null
  }
  const secretPart = token.slice(0, colonIdx)
  const userIdPart = token.slice(colonIdx + 1)
  if (!timingSafeStringEqual(secretPart, expectedSecret)) return null
  if (!isValidUserId(userIdPart)) return null
  return userIdPart
}

export async function signHs256Jwt(
  payload: Record<string, unknown>,
  secret: string,
): Promise<string> {
  const headerB64 = encodeBase64Url(TEXT_ENCODER.encode(JSON.stringify({ alg: 'HS256', typ: 'JWT' })))
  const payloadB64 = encodeBase64Url(TEXT_ENCODER.encode(JSON.stringify(payload)))
  const key = await crypto.subtle.importKey(
    'raw',
    TEXT_ENCODER.encode(secret),
    { name: 'HMAC', hash: 'SHA-256' },
    false,
    ['sign'],
  )
  const signingInput = TEXT_ENCODER.encode(`${headerB64}.${payloadB64}`)
  const sig = new Uint8Array(await crypto.subtle.sign('HMAC', key, signingInput))
  return `${headerB64}.${payloadB64}.${encodeBase64Url(sig)}`
}

export async function verifyHs256Jwt(
  token: string,
  secret: string,
): Promise<Record<string, unknown> | null> {
  const parts = token.split('.')
  if (parts.length !== 3) return null
  const [headerB64, payloadB64, signatureB64] = parts

  let header: Record<string, unknown>
  let payload: Record<string, unknown>
  let signatureBytes: Uint8Array
  try {
    header = JSON.parse(decodeBase64UrlToString(headerB64)) as Record<string, unknown>
    payload = JSON.parse(decodeBase64UrlToString(payloadB64)) as Record<string, unknown>
    signatureBytes = decodeBase64Url(signatureB64)
  } catch {
    return null
  }

  if (
    !header ||
    typeof header !== 'object' ||
    Array.isArray(header) ||
    !payload ||
    typeof payload !== 'object' ||
    Array.isArray(payload)
  ) {
    return null
  }

  if (header.alg !== 'HS256') return null

  const key = await crypto.subtle.importKey(
    'raw',
    TEXT_ENCODER.encode(secret),
    { name: 'HMAC', hash: 'SHA-256' },
    false,
    ['verify'],
  )
  const signingInput = TEXT_ENCODER.encode(`${headerB64}.${payloadB64}`)
  const valid = await crypto.subtle.verify('HMAC', key, signatureBytes, signingInput)
  if (!valid) return null

  if (typeof payload.exp !== 'number') return null
  const nowSec = Math.floor(Date.now() / 1000)
  if (payload.exp <= nowSec) return null

  return payload
}

export function timingSafeStringEqual(left: string, right: string): boolean {
  const a = TEXT_ENCODER.encode(left)
  const b = TEXT_ENCODER.encode(right)
  if (a.byteLength !== b.byteLength) return false
  let diff = 0
  for (let i = 0; i < a.byteLength; i++) {
    diff |= a[i] ^ b[i]
  }
  return diff === 0
}

export function encodeBase64Url(bytes: Uint8Array): string {
  let binary = ''
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i])
  }
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

export function decodeBase64Url(input: string): Uint8Array {
  const padded = input.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat((4 - (input.length % 4)) % 4)
  const binary = atob(padded)
  const bytes = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i)
  }
  return bytes
}

export function decodeBase64UrlToString(input: string): string {
  return TEXT_DECODER.decode(decodeBase64Url(input))
}

export async function sha256HexBytes(bytes: ArrayBuffer | Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', bytes)
  const arr = new Uint8Array(digest)
  let hex = ''
  for (let i = 0; i < arr.length; i++) {
    hex += arr[i].toString(16).padStart(2, '0')
  }
  return hex
}

export async function sha256HexString(input: string): Promise<string> {
  return sha256HexBytes(TEXT_ENCODER.encode(input))
}

export async function pkceS256Challenge(verifier: string): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', TEXT_ENCODER.encode(verifier))
  return encodeBase64Url(new Uint8Array(digest))
}

export function randomToken(byteLength = 32): string {
  const bytes = new Uint8Array(byteLength)
  crypto.getRandomValues(bytes)
  return encodeBase64Url(bytes)
}
