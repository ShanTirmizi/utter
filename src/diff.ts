// A word-by-word comparison of what was heard and what was written, for the
// "Changes" view of the last dictation.

export type Piece = { kind: "same" | "del" | "ins"; text: string };
/** One word of the result: usually a single piece, two when a word changed. */
export type Word = Piece[];

/** Compare words by their letters and digits only, so "Thursday." matches "thursday". */
const bare = (word: string) => word.toLowerCase().replace(/[^\p{L}\p{N}]/gu, "");

export function diffWords(heard: string, written: string): Word[] {
  const a = heard.split(/\s+/).filter(Boolean);
  const b = written.split(/\s+/).filter(Boolean);
  const na = a.map(bare);
  const nb = b.map(bare);

  // Longest common subsequence, filled from the end so it can be read forwards.
  const lcs = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0));
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      lcs[i][j] = na[i] !== "" && na[i] === nb[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    }
  }

  const words: Word[] = [];
  const push = (kind: "del" | "ins", text: string) => {
    // Run neighbouring removals (or additions) together: "um so", not "um" "so".
    const last = words[words.length - 1];
    if (last && last.length === 1 && last[0].kind === kind) last[0].text += ` ${text}`;
    else words.push([{ kind, text }]);
  };

  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    if (na[i] !== "" && na[i] === nb[j]) {
      if (a[i] === b[j]) {
        words.push([{ kind: "same", text: b[j] }]);
      } else if (b[j].startsWith(a[i])) {
        // Only punctuation was added: "know" became "know."
        words.push([
          { kind: "same", text: a[i] },
          { kind: "ins", text: b[j].slice(a[i].length) },
        ]);
      } else {
        words.push([
          { kind: "del", text: a[i] },
          { kind: "ins", text: b[j] },
        ]);
      }
      i++;
      j++;
    } else if (lcs[i + 1][j] >= lcs[i][j + 1]) {
      push("del", a[i++]);
    } else {
      push("ins", b[j++]);
    }
  }
  while (i < a.length) push("del", a[i++]);
  while (j < b.length) push("ins", b[j++]);
  return words;
}
