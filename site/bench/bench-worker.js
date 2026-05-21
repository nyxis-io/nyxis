// Worker: sum score over a record index range (used by bench page).
import { NxsReader } from "/sdk/nxs.js";

self.onmessage = (ev) => {
  const msg = ev.data;
  if (msg.type !== "sum-chunk") return;
  const bytes = new Uint8Array(msg.buffer);
  const r = new NxsReader(bytes);
  let sum = 0;
  for (let i = msg.start; i < msg.end; i++) sum += r.record(i).getF64("score");
  self.postMessage({ type: "sum-result", sum, workerId: msg.workerId });
};
