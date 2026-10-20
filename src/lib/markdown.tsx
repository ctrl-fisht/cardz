import { useEffect, useMemo, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { api } from "./api";

const mediaCache = new Map<string, string>();
const MEDIA_RE = /!\[[^\]]*\]\((media\/[^)\s]+)(?:\s+"[^"]*")?\)/g;

function extractMedia(text: string): string[] {
  const found = new Set<string>();
  for (const match of text.matchAll(MEDIA_RE)) {
    if (match[1]) found.add(match[1]);
  }
  return [...found];
}

export function Markdown({
  text,
  deckId,
  className = "",
}: {
  text: string;
  deckId: string;
  className?: string;
}) {
  const links = useMemo(() => extractMedia(text), [text]);
  const linksKey = links.join("|");
  const [urls, setUrls] = useState<Record<string, string>>({});

  useEffect(() => {
    let alive = true;
    if (links.length === 0) {
      setUrls({});
      return;
    }
    Promise.all(
      links.map(async (rel) => {
        const cacheKey = `${deckId}|${rel}`;
        const cached = mediaCache.get(cacheKey);
        if (cached) return [rel, cached] as const;
        const url = await api.readMedia(deckId, rel).catch(() => "");
        if (url) mediaCache.set(cacheKey, url);
        return [rel, url] as const;
      }),
    )
      .then((pairs) => {
        if (alive) setUrls(Object.fromEntries(pairs.filter(([, url]) => url)));
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [deckId, linksKey]);

  return (
    <div className={`md ${className}`}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        urlTransform={(url) => urls[url] ?? url}
        components={{
          a: ({ href, children }) => (
            <a href={href} target="_blank" rel="noreferrer">
              {children}
            </a>
          ),
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
}
