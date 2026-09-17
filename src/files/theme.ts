// CodeMirror theme on the app's design tokens (styles.css), plus the mockup's
// highlight palette: keywords accent, strings ok, comments muted, numbers --num.
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { tags as t } from "@lezer/highlight";

export const viewerTheme = EditorView.theme(
  {
    "&": { height: "100%", backgroundColor: "var(--bg)", color: "var(--text)" },
    ".cm-scroller": { fontFamily: "var(--mono)", fontSize: "13px", lineHeight: "1.65", padding: "12px 0" },
    ".cm-content": { tabSize: "2", padding: "0" },
    ".cm-gutters": { backgroundColor: "var(--bg)", color: "var(--muted)", border: "0" },
    ".cm-lineNumbers .cm-gutterElement": { minWidth: "52px", padding: "0 18px 0 0", fontVariantNumeric: "tabular-nums" },
    ".cm-line": { padding: "0 16px 0 0" },
    "&.cm-focused": { outline: "none" },
    ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": { backgroundColor: "var(--sel)" },
    ".cm-searchMatch": { backgroundColor: "var(--shell-soft)", outline: "1px solid var(--shell)" },
    ".cm-searchMatch.cm-searchMatch-selected": { backgroundColor: "var(--agent-soft)" },
    ".cm-panels": { backgroundColor: "var(--panel)", color: "var(--text-2)", borderColor: "var(--line)" },
    ".cm-panels.cm-panels-top": { borderBottom: "1px solid var(--line)" },
    ".cm-panel.cm-search": { fontFamily: "var(--sans)", fontSize: "12.5px", padding: "6px 12px" },
    ".cm-panel.cm-search input, .cm-panel.cm-search button": { font: "inherit", color: "var(--text)", background: "var(--bg-sunk)", border: "1px solid var(--line-strong)", borderRadius: "5px", padding: "3px 8px" },
    ".cm-panel.cm-search label": { marginLeft: "8px" },
    ".cm-specialChar": { color: "var(--agent)" },
  },
  { dark: true },
);

const highlight = HighlightStyle.define([
  { tag: [t.keyword, t.modifier, t.operatorKeyword, t.controlKeyword, t.definitionKeyword], color: "var(--accent)" },
  { tag: [t.string, t.special(t.string), t.regexp], color: "var(--ok)" },
  { tag: [t.comment, t.lineComment, t.blockComment, t.docComment], color: "var(--muted)", fontStyle: "italic" },
  { tag: [t.number, t.bool, t.null, t.atom], color: "var(--num)" },
  { tag: t.heading, color: "var(--text)", fontWeight: "600" },
  { tag: [t.typeName, t.className, t.namespace], color: "var(--text)" },
  { tag: [t.propertyName, t.attributeName, t.tagName], color: "var(--text-2)" },
  { tag: t.monospace, color: "var(--ok)" },
  { tag: [t.punctuation, t.separator, t.bracket], color: "var(--text-2)" },
]);

export const viewerHighlight = syntaxHighlighting(highlight);
