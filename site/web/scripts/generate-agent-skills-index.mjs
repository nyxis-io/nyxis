#!/usr/bin/env node
/**
 * Regenerate /.well-known/agent-skills/index.json with SHA-256 digests.
 * Run before vite build (see package.json).
 */
import { createHash } from "node:crypto";
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const skillsDir = resolve(import.meta.dirname, "../public/.well-known/agent-skills");
const outPath = join(skillsDir, "index.json");

const skills = readdirSync(skillsDir)
  .filter((name) => name.endsWith(".md"))
  .sort()
  .map((filename) => {
    const body = readFileSync(join(skillsDir, filename), "utf8");
    const digest = createHash("sha256").update(body).digest("hex");
    const front = body.match(/^---\n([\s\S]*?)\n---/);
    const meta = Object.fromEntries(
      (front?.[1] ?? "")
        .split("\n")
        .filter(Boolean)
        .map((line) => {
          const i = line.indexOf(":");
          return [line.slice(0, i).trim(), line.slice(i + 1).trim()];
        }),
    );
    const name = meta.name ?? filename.replace(/\.md$/, "");
    return {
      name,
      type: "skill-md",
      description: meta.description ?? "",
      url: `https://nyxis.io/.well-known/agent-skills/${filename}`,
      digest: `sha256:${digest}`,
    };
  });

const index = {
  $schema: "https://schemas.agentskills.io/discovery/0.2.0/schema.json",
  skills,
};

writeFileSync(outPath, `${JSON.stringify(index, null, 2)}\n`);
console.log(`Wrote ${outPath} (${skills.length} skills)`);
