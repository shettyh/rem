import { describe, it, expect } from 'vitest'
import { FakeGitBridge } from './FakeGitBridge'

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
