export function naturalizeSvg(container: HTMLElement | null): void {
  const svg = container?.querySelector("svg");
  if (!svg) return;
  const box = svg.viewBox?.baseVal;
  if (box && box.width > 0 && box.height > 0) {
    svg.style.maxWidth = "none";
    svg.setAttribute("width", String(box.width));
    svg.setAttribute("height", String(box.height));
  }
}
