import type { Template } from "./types";

export function renderTemplate(
  source: string,
  fields: Record<string, string>,
  frontSide = "",
): string {
  let out = source.replace(
    /\{\{#(\w+)\}\}([\s\S]*?)\{\{\/\1\}\}/g,
    (_match, key: string, inner: string) =>
      (fields[key.toLowerCase()] ?? "").trim() ? inner : "",
  );
  out = out.replace(/\{\{\s*FrontSide\s*\}\}/gi, frontSide);
  out = out.replace(
    /\{\{\s*(\w+)\s*\}\}/g,
    (_match, key: string) => fields[key.toLowerCase()] ?? "",
  );
  return out.trim();
}

export function fieldLabel(
  template: Template | undefined,
  field: string,
): string {
  const custom = template?.labels?.[field];
  if (custom) return custom;
  const base = field.replace(/[_-]+/g, " ");
  return base.charAt(0).toUpperCase() + base.slice(1);
}

export function blankFields(template: Template): Record<string, string> {
  return Object.fromEntries(template.fields.map((field) => [field, ""]));
}
