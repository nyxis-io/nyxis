// JSON worker — receives a full structured-cloned copy of the parsed array.
// The postMessage structured clone is the "copy cost" we're measuring against
// NXS's zero-copy SAB share.

let data = null;

self.onmessage = (ev) => {
  const msg = ev.data;

  if (msg.type === "init") {
    const t0 = performance.now();
    data = msg.data; // structured clone ran on transit; this assignment is cheap
    const t1 = performance.now();
    self.postMessage({
      type: "ready",
      workerId: msg.workerId,
      initMs: t1 - t0, // this is just the assignment; the real cost is on the sender
      recordCount: Array.isArray(data) ? data.length : 0,
    });
    return;
  }

  if (msg.type === "read") {
    const { index, key, requestId } = msg;
    const value = data && data[index] ? data[index][key] : undefined;
    self.postMessage({ type: "read-result", requestId, index, key, value });
    return;
  }
};
