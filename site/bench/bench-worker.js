// Worker: sum score over a record index range (used by bench page).
import { NxsReader } from "/sdk/nxs.js";

self.onmessage = (ev) => {
  const msg = ev.data;
  if (msg?.type !== "sum-chunk") return;
  try {
    const bytes = new Uint8Array(msg.buffer);
    const r = new NxsReader(bytes);
    const scoreSlot = r.slot("score");
    const cur = r.cursor();
    let sum = 0;
    for (let i = msg.start; i < msg.end; i++) {
      cur.seek(i);
      sum += cur.getF64BySlot(scoreSlot);
    }
    self.postMessage({ type: "sum-result", sum, workerId: msg.workerId });
  } catch (e) {
    self.postMessage({ type: "sum-error", message: e?.message ?? String(e), workerId: msg.workerId });
  }
};
