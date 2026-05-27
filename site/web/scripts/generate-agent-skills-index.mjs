#!/usr/bin/env node
/**
 * Regenerate agent build artifacts: skills index and nginx markdown token header.
 * Run before vite build (see package.json).
 */
import { createHash } from "node:crypto";
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { loadSiteConfig } from "./lib/page-html.mjs";

const webRoot = resolve(import.meta.dirname, "..");
const siteOrigin = loadSiteConfig(webRoot).origin;
const skillsDir = join(webRoot, "public/.well-known/agent-skills");
const outPath = join(skillsDir, "index.json");
const indexMdPath = join(webRoot, "public/index.md");
const markdownTokensPath = resolve(webRoot, "../../docker/markdown-tokens.conf");

function approxMarkdownTokens(text) {
  return Math.ceil(text.split(/\s+/).filter(Boolean).length * 1.33);
}

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
          return i === -1 ? null : [line.slice(0, i).trim(), line.slice(i + 1).trim()];
        })
        .filter(Boolean),
    );
    const name = meta.name ?? filename.replace(/\.md$/, "");
    return {
      name,
      type: "skill-md",
      description: meta.description ?? "",
      url: `${siteOrigin}/.well-known/agent-skills/${filename}`,
      digest: `sha256:${digest}`,
    };
  });

const index = {
  $schema: "https://schemas.agentskills.io/discovery/0.2.0/schema.json",
  skills,
};

writeFileSync(outPath, `${JSON.stringify(index, null, 2)}\n`);
console.log(`Wrote ${outPath} (${skills.length} skills)`);

const indexMd = readFileSync(indexMdPath, "utf8");
const tokens = approxMarkdownTokens(indexMd);
writeFileSync(markdownTokensPath, `add_header x-markdown-tokens "${tokens}" always;\n`);
console.log(`Wrote ${markdownTokensPath} (${tokens} tokens)`);

const robotsPath = join(webRoot, "public/robots.txt");
const legacyRobotsPath = resolve(webRoot, "../../site/robots.txt");
const robotsBody = readFileSync(robotsPath, "utf8");
writeFileSync(legacyRobotsPath, robotsBody);
console.log(`Synced ${legacyRobotsPath}`);
