import { describe, it, expect } from 'vitest'
import { FakeGitBridge } from './FakeGitBridge'
import type { AssetBlob } from './snapshot'

describe('FakeGitBridge', () => {
  it('reports an empty remote until first push', async () => {
    const b = new FakeGitBridge(null)
    await b.clone('url', 'dir')
    const { remoteExists } = await b.fetchReset('dir')
    expect(remoteExists).toBe(false)
    await b.writeFiles('dir', { 'rem.json': '{}' })
    const res = await b.commitPush('dir', 'msg')
    expect(res).toEqual({ pushed: true, rejected: false })
    expect(b.remote).toEqual({ 'rem.json': '{}' })
  })

  it('rejects a push when the remote advanced mid-sync, then accepts on retry', async () => {
    const b = new FakeGitBridge({ 'rem.json': '{}' })
    await b.clone('url', 'dir')
    await b.fetchReset('dir')
    b.pushInterceptor = () => { b.remote = { 'rem.json': '{}', 'x': '1' }; b.bumpRemote() }
    await b.writeFiles('dir', { 'rem.json': '{}', 'mine': '2' })
    expect(await b.commitPush('dir', 'm')).toEqual({ pushed: false, rejected: true })
    await b.fetchReset('dir')
    await b.writeFiles('dir', { 'rem.json': '{}', 'merged': '3' })
    expect(await b.commitPush('dir', 'm')).toEqual({ pushed: true, rejected: false })
  })
})

const blob = (h: string): AssetBlob => ({ hash: h, mime: 'image/png', bytes: new Uint8Array([1, 2]) })

describe('FakeGitBridge assets', () => {
  it('writes and reads assets in the working copy', async () => {
    const bridge = new FakeGitBridge(null)
    await bridge.clone('url', '/d')
    await bridge.writeAssets('/d', [blob('a'.repeat(64))])
    const read = await bridge.readAssets('/d')
    expect(read.map((a) => a.hash)).toEqual(['a'.repeat(64)])
  })

  it('replaces the asset set on write (delete-absent)', async () => {
    const bridge = new FakeGitBridge(null)
    await bridge.clone('url', '/d')
    await bridge.writeAssets('/d', [blob('a'.repeat(64)), blob('b'.repeat(64))])
    await bridge.writeAssets('/d', [blob('a'.repeat(64))])
    expect((await bridge.readAssets('/d')).map((a) => a.hash)).toEqual(['a'.repeat(64)])
  })

  it('publishes assets to the remote on commitPush', async () => {
    const bridge = new FakeGitBridge(null)
    await bridge.clone('url', '/d')
    await bridge.fetchReset('/d')
    await bridge.writeAssets('/d', [blob('a'.repeat(64))])
    await bridge.commitPush('/d', 'msg')
    expect(bridge.remoteAssets.map((a) => a.hash)).toEqual(['a'.repeat(64)])
  })
})
