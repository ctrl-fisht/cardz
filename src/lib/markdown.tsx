import { useEffect, useMemo, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { api } from "./api";
import { parseImageWidth } from "./media";

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
  editable = false,
  onResize,
}: {
  text: string;
  deckId: string;
  className?: string;
  editable?: boolean;
  onResize?: (src: string, width: string) => void;
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
  }, [deckId, linksKey]);

  const sourceByUrl = useMemo(() => {
    const map = new Map<string, string>();
    for (const [relative, resolved] of Object.entries(urls)) {
      map.set(resolved, relative);
    }
    return map;
  }, [urls]);

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
          img: ({ src, alt, title }) => {
            const width = parseImageWidth(title);
            const original = (src && sourceByUrl.get(src)) || src || "";
            const image = (
              <img
                src={src}
                alt={alt ?? ""}
                style={width ? { width } : undefined}
              />
            );
            if (!editable || !original) return image;
            return (
              <span className="md-image">
                {image}
                <span className="image-tools">
                  {["25%", "50%", "75%", "100%"].map((option) => (
                    <button
                      key={option}
                      type="button"
                      className={`image-size-btn ${
                        width === option ? "on" : ""
                      }`}
                      onClick={() => onResize?.(original, option)}
                    >
                      {option.replace("%", "")}
                    </button>
                  ))}
                  <span className="image-size-label">{width ?? "авто"}</span>
                </span>
              </span>
            );
          },
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
}
