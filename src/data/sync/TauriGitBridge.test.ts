import { beforeEach, describe, expect, it, vi } from 'vitest'
import { TauriGitBridge } from './TauriGitBridge'

const invoke = vi.hoisted(() => vi.fn())

vi.mock('@tauri-apps/api/core', () => ({ invoke }))

describe('TauriGitBridge', () => {
  beforeEach(() => invoke.mockReset())

  it('does not expose or send a caller-selected repository directory', async () => {
    invoke.mockResolvedValueOnce(false)

    await new TauriGitBridge().isCloned()

    expect(invoke).toHaveBeenCalledWith('git_is_cloned')
  })
})
