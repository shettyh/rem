import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { RichMarkdownEditor } from './RichMarkdownEditor'

describe('RichMarkdownEditor', () => {
  it('renders provided markdown content', async () => {
    render(<RichMarkdownEditor value="hello **world**" onChange={() => {}} ariaLabel="Front" />)
    expect(await screen.findByText('world')).toBeInTheDocument()
  })

  it('mounts with an empty value without crashing', () => {
    const { container } = render(<RichMarkdownEditor value="" onChange={() => {}} />)
    expect(container.querySelector('.rich-editor')).toBeTruthy()
  })
})
