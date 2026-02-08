/**
 * Story Collector
 * Test file for reproducing UTF-8 character boundary parsing error
 */
export class StoryCollector {
  /**
   * Build tree representation lines
   * This method uses Unicode box-drawing characters that trigger the parsing error
   */
  private buildTreeLines(
    info: StoryInfo,
    hierarchy: Map<string, StoryInfo>,
    lines: string[],
    prefix: string,
    isLast: boolean
  ): void {
    // These Unicode box-drawing characters cause UTF-8 boundary errors:
    // └── (U+2514 U+2500 U+2500) = 9 bytes
    // ├── (U+251C U+2500 U+2500) = 9 bytes
    // │   (U+2502) = 3 bytes
    const connector = isLast ? '└── ' : '├── ';
    const line = `${prefix}${connector}${info.selfId} (from: ${info.parentElement})`;
    lines.push(line);

    // This also uses the problematic │ character
    const childPrefix = prefix + (isLast ? '    ' : '│   ');
    const children = info.childStories
      .map(id => hierarchy.get(id))
      .filter(child => child !== undefined) as StoryInfo[];

    for (let i = 0; i < children.length; i++) {
      this.buildTreeLines(children[i], hierarchy, lines, childPrefix, i === children.length - 1);
    }
  }

  /**
   * Example of more Unicode box-drawing characters in comments and strings
   * ┌───────────────┐
   * │ Tree Example  │
   * ├─── Child 1    │
   * ├─── Child 2    │
   * └─── Child 3    │
   * └───────────────┘
   */
  private renderTree(): string {
    const tree = `
    Root
    ├── Branch 1
    │   ├── Leaf 1.1
    │   └── Leaf 1.2
    └── Branch 2
        ├── Leaf 2.1
        └── Leaf 2.2
    `;
    return tree;
  }
}

// Type definitions for testing
interface StoryInfo {
  selfId: string;
  parentElement: string;
  childStories: string[];
}