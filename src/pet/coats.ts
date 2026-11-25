export type CoatPattern = "solid" | "tabby" | "calico" | "point";

export interface Coat {
  id: string;
  name: string;
  fur: string;
  paw: string;
  inner: string;
  eye: string;
  pupil: string;
  whisker: string;
  outline: string;
  line: string;
  sock: number;
  pattern: CoatPattern;
  stripe?: string;
  patchA?: string;
  patchB?: string;
  muzzle?: string;
  blush?: string;
  tail?: string;
}

export const COATS: Coat[] = [
  {
    id: "tuxedo",
    name: "奶牛猫",
    fur: "#161616",
    paw: "#f8f5ea",
    inner: "#2a2521",
    eye: "#e7bd3a",
    pupil: "#0c0c0c",
    whisker: "#f3f0e4",
    outline: "#f1eee2",
    line: "#e7bd3a",
    sock: 0.12,
    pattern: "solid",
  },
  {
    id: "orange",
    name: "橘猫",
    fur: "#e9973f",
    paw: "#fbe8c9",
    inner: "#d98a86",
    eye: "#6b9440",
    pupil: "#161713",
    whisker: "#fffaf0",
    outline: "#f6e7cf",
    line: "#7a4312",
    sock: 0.2,
    pattern: "tabby",
    stripe: "#c26a1f",
    muzzle: "#fbe8c9",
    blush: "#f3a39a",
  },
  {
    id: "tabby",
    name: "狸花猫",
    fur: "#a08a6c",
    paw: "#d7c6a8",
    inner: "#d9a79a",
    eye: "#8fae4a",
    pupil: "#14150f",
    whisker: "#f4f0e4",
    outline: "#efe8d8",
    line: "#3f3324",
    sock: 0.18,
    pattern: "tabby",
    stripe: "#4d3d2b",
    muzzle: "#d7c6a8",
    blush: "#dfa89f",
  },
  {
    id: "calico",
    name: "三花猫",
    fur: "#f8f6ef",
    paw: "#f8f6ef",
    inner: "#f2b9b0",
    eye: "#c79a2e",
    pupil: "#141310",
    whisker: "#b5b1a2",
    outline: "#b5b1a2",
    line: "#8a8676",
    sock: 0.16,
    pattern: "calico",
    patchA: "#e9973f",
    patchB: "#1d1c1a",
    muzzle: "#fbf8f0",
    blush: "#f2b0a8",
    tail: "#1d1c1a",
  },
  {
    id: "white",
    name: "白猫",
    fur: "#f8f6ef",
    paw: "#f8f6ef",
    inner: "#f2b9b0",
    eye: "#5aa9c9",
    pupil: "#111310",
    whisker: "#b5b1a2",
    outline: "#b5b1a2",
    line: "#8a8676",
    sock: 0.16,
    pattern: "solid",
    muzzle: "#fffdf7",
    blush: "#f4b8b0",
  },
  {
    id: "blue",
    name: "蓝灰猫",
    fur: "#8090a0",
    paw: "#cdd5da",
    inner: "#c59aa0",
    eye: "#e0a63a",
    pupil: "#121412",
    whisker: "#f4f4ee",
    outline: "#e7ecef",
    line: "#3c4650",
    sock: 0.18,
    pattern: "solid",
    muzzle: "#cdd5da",
    blush: "#e3a9a4",
  },
  {
    id: "siamese",
    name: "暹罗猫",
    fur: "#f4ead9",
    paw: "#4a352b",
    inner: "#7d5e52",
    eye: "#5c8fbe",
    pupil: "#0e1113",
    whisker: "#fbf6ea",
    outline: "#b5b1a2",
    line: "#6b5548",
    sock: 0.3,
    pattern: "point",
    tail: "#4a352b",
  },
];

export function coatById(id: string): Coat {
  return COATS.find((c) => c.id === id) ?? COATS[0];
}