const SYSTEM_TAGS = new Set(['leech'])

export function isSystemTag(tag: string): boolean {
  return SYSTEM_TAGS.has(tag)
}

export function userTags(tags: string[]): string[] {
  return tags.filter((tag) => !isSystemTag(tag))
}

/** Parse comma-separated user tags, preserving first spelling and removing duplicates. */
export function parseUserTags(input: string): string[] {
  const seen = new Set<string>()
  const tags: string[] = []

  for (const part of input.split(',')) {
    const tag = part.trim()
    const key = tag.toLocaleLowerCase()
    if (!tag || SYSTEM_TAGS.has(key) || seen.has(key)) continue
    seen.add(key)
    tags.push(tag)
  }

  return tags
}

/** Replace user tags while retaining system-owned card metadata such as `leech`. */
export function mergeUserTags(existing: string[], input: string): string[] {
  return [...existing.filter(isSystemTag), ...parseUserTags(input)]
}
