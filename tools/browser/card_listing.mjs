// The lab's JS card reader (CrystalOSD tools/mc/read_card.mjs) on card images, as a listing the Rust reader is compared with
// (crates/assets/tests/memory_card.rs).
//   node tools/browser/card_listing.mjs <out.json> <image.ps2>...
// Per image: info(), the superblock, the ECC report, the FAT's length and FNV-1a hash of its entries, and the root directory's entries.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const LAB = path.resolve(process.env.CRYSTALOSD_ROOT ?? path.join(HERE, '../../../CrystalOSD'));
const { info, superblock, fat, directory, eccReport } = await import(path.join(LAB, 'tools/mc/read_card.mjs'));

const fnv = (words) => {
  let h = 0x811c9dc5;
  for (const w of words) for (let k = 0; k < 4; k++) { h ^= (w >>> (8 * k)) & 0xff; h = Math.imul(h, 0x01000193) >>> 0; }
  return h;
};
const [out, ...files] = process.argv.slice(2);
const listing = {};
for (const file of files) {
  const image = fs.readFileSync(file);
  const sb = superblock(image);
  const entry = { info: info(image), superblock: sb, ecc: eccReport(image) };
  if (sb) {
    const table = fat(image, sb);
    entry.fat = { length: table.length, hash: fnv(table) };
    entry.root = directory(image, sb, table, sb.rootdirCluster, entry.info.rootEntries);
  }
  listing[path.basename(file)] = entry;
}
fs.writeFileSync(out, JSON.stringify(listing, null, 1));
console.log(`${files.length} images -> ${out}`);
