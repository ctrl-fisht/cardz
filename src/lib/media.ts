const WIDTH_RE = /^w=(\d{1,3}%|\d{2,4}px)$/;

export function parseImageWidth(title?: string | null): string | undefined {
  if (!title) return undefined;
  const match = WIDTH_RE.exec(title.trim());
  return match ? match[1] : undefined;
}

export function rewriteImageWidth(
  text: string,
  src: string,
  width: string,
): string {
  const imageRe = /!\[([^\]]*)\]\(([^)\s]+)(?:\s+"([^"]*)")?\)/g;
  return text.replace(
    imageRe,
    (match: string, alt: string, url: string, _title?: string) => {
      if (url !== src) return match;
      return width === "auto"
        ? `![${alt}](${url})`
        : `![${alt}](${url} "w=${width}")`;
    },
  );
}
