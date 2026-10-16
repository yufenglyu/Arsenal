import type { Options } from "react-markdown";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import "katex/dist/katex.min.css";

export const mathRemarkPlugins: NonNullable<Options["remarkPlugins"]> = [remarkMath];
// Render locally, without trusted HTML/URLs or unbounded macro expansion.
export const mathRehypePlugins: NonNullable<Options["rehypePlugins"]> = [
  [rehypeKatex, { trust: false, maxExpand: 1000, maxSize: 20 }],
];
