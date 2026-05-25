# Documentation — Nyxis quickstart

5-minute path to first interaction in the browser with the MIT JavaScript reader.

## Install

Copy `nxs.js` from [nyxis-drivers/js](https://github.com/nyxis-io/nyxis-drivers/tree/main/js) or serve from `/sdk/nxs.js`.

## Open, filter, render

```js
import { NxsReader, NxsStreamReader } from "./nxs.js";

const reader = await NxsReader.open("/data/logs.nxb");
const cur = reader.cursor();
for (let i = 0; i < reader.recordCount; i++) {
  cur.seek(i);
  if (cur.getF64("score") > 80) { /* … */ }
}
```

## Next steps

- [Log explorer](https://nyxis.io/demo/explorer)
- [Specification](https://github.com/nyxis-io/nyxis/blob/main/SPEC.md)
- [Use cases](https://nyxis.io/use-cases/)
