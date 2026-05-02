#!/usr/bin/env node
import { copyFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const packageRoot = path.resolve(__dirname, "..");
const repoRoot = path.resolve(packageRoot, "..");
const generatedFiles = [
  ["config.toml", "config.toml"],
  [".env.example", ".env.example"],
  ["README.md", "README.md"],
  ["LICENSE", "LICENSE"],
  ["NOTICE", "NOTICE"],
];

for (const [sourceName, destinationName] of generatedFiles) {
  const source = path.join(repoRoot, sourceName);
  const destination = path.join(packageRoot, destinationName);

  if (!existsSync(source)) {
    throw new Error(`Missing package source file: ${source}`);
  }

  copyFileSync(source, destination);
}
