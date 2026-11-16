// An icon's matrices (HDD OSD 1.10U), func_00259240 0x00259240: the item's local matrix from its record, and matrix A, the row's
// matrix (row + 0x30) times the local one, which func_002567D0 sends to VU1.
//   angles: item + 0, + 4, + 8, each through func_0024DFC8 (the wrap below); position: item + 0x10
//   kinds 1 and 0xC (item + 0x130): position y - D_0036FEFC, the y angle wrap(item + 4 + 0.0)
//   kind 0: the y angle wrap(wrap(item + 4) + D_0036FF04)
//   sceVu0RotMatrix of the unit matrix, sceVu0TransMatrix, then func_0024DCE0 scales rows 0..2 by item + 0x60 x func_00264498()
//     (the probed f12: kinds 1 and 0xC have multiplied item + 0x60 by D_0036FF00 first)
//   sceVu0MulMatrix(item + 0x20, row + 0x30, local)
import { f, add, sub, unit, rotMatrix, transMatrix, mulMatrix, matrix, vector, toInt } from '../../../CrystalOSD/References/model/opening-lib.mjs';

export const CONSTANTS = (read) => {
  const c = (a) => read(a, 4).readFloatLE(0);
  return { gt: c(0x0036fdc0), lt: c(0x0036fdc4), period: c(0x0036fdc8), hi: c(0x0036fdcc), hiSub: c(0x0036fdd0), lo: c(0x0036fdd4), loAdd: c(0x0036fdd8),
    drop: c(0x0036fefc), decay: c(0x0036ff00), spin: c(0x0036ff04) };
};

// func_0024DFC8: an angle beyond D_0036FDC0 (or below -D_0036FDC4) loses whole periods of D_0036FDC8 (the quotient truncated);
// then above D_0036FDCC it loses D_0036FDD0, below D_0036FDD4 it gains D_0036FDD8.
export function wrap(a, K) {
  if ((a > 0 && K.gt < a) || (!(a > 0) && K.lt < -a)) a = sub(a, f(toInt(f(a / K.period)) * K.period));
  if (K.hi < a) return sub(a, K.hiSub);
  if (a < K.lo) return add(a, K.loAdd);
  return a;
}

export function localMatrix(item, scale, K) {
  const kind = item.readInt32LE(0x130);
  const angles = [wrap(item.readFloatLE(0), K), wrap(item.readFloatLE(4), K), wrap(item.readFloatLE(8), K), 1];
  const position = vector(item, 0x10);
  if (kind === 1 || kind === 0xc) {
    position[1] = sub(position[1], K.drop);
    angles[1] = wrap(add(item.readFloatLE(4), 0), K);
  } else if (kind === 0) angles[1] = wrap(add(angles[1], K.spin), K);
  const m = transMatrix(rotMatrix(unit(), angles), position);
  return m.map((row, i) => (i < 3 ? row.map((v) => f(v * scale)) : row));
}

export const itemMatrix = (row, local) => mulMatrix(matrix(row, 0x30), local);
