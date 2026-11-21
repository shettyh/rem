import type { Env } from './types'

const USER_ID_RE = /^[a-zA-Z0-9_-]{1,64}$/
const TEXT_ENCODER = new TextEncoder()

export interface AuthContext {
  userId: string
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
    const userId = await verifyHs256Jwt(token, env.JWT_SECRET)
    if (userId) return { userId }
  }

  if (env.DEV_AUTH_TOKEN && env.DEV_AUTH_TOKEN.length > 0) {
    const userId = verifyDevToken(token, env.DEV_AUTH_TOKEN)
    if (userId) return { userId }
  }

  return null
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

async function verifyHs256Jwt(token: string, secret: string): Promise<string | null> {
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

  if (typeof payload.sub !== 'string' || !isValidUserId(payload.sub)) return null
  if (typeof payload.exp !== 'number') return null
  const nowSec = Math.floor(Date.now() / 1000)
  if (payload.exp <= nowSec) return null

  return payload.sub
}

function timingSafeStringEqual(left: string, right: string): boolean {
  const a = TEXT_ENCODER.encode(left)
  const b = TEXT_ENCODER.encode(right)
  if (a.byteLength !== b.byteLength) return false
  let diff = 0
  for (let i = 0; i < a.byteLength; i++) {
    diff |= a[i] ^ b[i]
  }
  return diff === 0
}

function decodeBase64Url(input: string): Uint8Array {
  const padded = input.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat((4 - (input.length % 4)) % 4)
  const binary = atob(padded)
  const bytes = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i)
  }
  return bytes
}

function decodeBase64UrlToString(input: string): string {
  return new TextDecoder().decode(decodeBase64Url(input))
}
