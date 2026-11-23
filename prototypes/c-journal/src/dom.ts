export function el(
  tag: string,
  attrs: Record<string, string> | null,
  ...children: Array<Node | string | null | false>
): HTMLElement {
  const node = document.createElement(tag);
  if (attrs) {
    for (const [key, value] of Object.entries(attrs)) {
      if (key === "class") {
        node.className = value;
      } else {
        node.setAttribute(key, value);
      }
    }
  }
  for (const child of children) {
    if (child === null || child === false || child === "") {
      continue;
    }
    node.append(typeof child === "string" ? document.createTextNode(child) : child);
  }
  return node;
}
