// A word-by-word comparison of what was heard and what was written, for the
// "Changes" view of the last dictation.

/** "break" is a line break in what was written: a new line or paragraph. */
export type Piece = { kind: "same" | "del" | "ins" | "break"; text: string };
/** One word of the result: usually a single piece, two when a word changed. */
export type Word = Piece[];

/** Curly quotes and apostrophes read the same as straight ones. */
const plainQuotes = (text: string) => text.replace(/[‘’ʼ]/g, "'").replace(/[“”]/g, '"');

/** Compare words by their letters and digits only, so "Thursday." matches "thursday". */
const bare = (word: string) => word.toLowerCase().replace(/[^\p{L}\p{N}]/gu, "");

/** Words, with each run of line breaks in between kept as a token of its own. */
const tokens = (text: string) => text.split(/(\n+)|[^\S\n]+/).filter((t): t is string => !!t);

export function diffWords(heard: string, written: string): Word[] {
  // What was heard has no layout worth showing; what was written does.
  const a = heard.split(/\s+/).filter(Boolean);
  const b = tokens(written);
  const na = a.map(bare);
  const nb = b.map((t) => (t.startsWith("\n") ? "" : bare(t)));

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
  const lineBreak = (text: string) => words.push([{ kind: "break", text }]);

  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    if (b[j].startsWith("\n")) {
      lineBreak(b[j++]);
    } else if (na[i] !== "" && na[i] === nb[j]) {
      words.push(changedWord(a[i], b[j]));
      i++;
      j++;
    } else if (lcs[i + 1][j] >= lcs[i][j + 1]) {
      push("del", a[i++]);
    } else {
      push("ins", b[j++]);
    }
  }
  while (i < a.length) push("del", a[i++]);
  while (j < b.length) {
    if (b[j].startsWith("\n")) lineBreak(b[j++]);
    else push("ins", b[j++]);
  }
  return words;
}

/**
 * The same word, heard one way and written another. Show only what really
 * changed: nothing for a curly apostrophe, the comma alone for "Friday,".
 */
function changedWord(heard: string, written: string): Word {
  const was = plainQuotes(heard);
  const now = plainQuotes(written);
  if (was === now) return [{ kind: "same", text: written }];
  if (now.startsWith(was)) {
    // Only punctuation was added: "know" became "know."
    return [
      { kind: "same", text: written.slice(0, heard.length) },
      { kind: "ins", text: written.slice(heard.length) },
    ];
  }
  if (was.startsWith(now)) {
    // Only punctuation was taken off: "ideas." became "ideas"
    return [
      { kind: "same", text: written },
      { kind: "del", text: heard.slice(written.length) },
    ];
  }
  // The same word with different punctuation after it: "ideas." became "ideas,".
  const tail = /[^\p{L}\p{N}']+$/u;
  const wasCore = was.replace(tail, "");
  if (wasCore && wasCore === now.replace(tail, "")) {
    const pieces: Piece[] = [{ kind: "same", text: written.slice(0, wasCore.length) }];
    if (heard.length > wasCore.length) pieces.push({ kind: "del", text: heard.slice(wasCore.length) });
    if (written.length > wasCore.length) pieces.push({ kind: "ins", text: written.slice(wasCore.length) });
    return pieces;
  }
  return [
    { kind: "del", text: heard },
    { kind: "ins", text: written },
  ];
}
