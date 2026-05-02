#!/usr/bin/env node
import { existsSync, readFileSync, unlinkSync } from "node:fs";
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

for (const [sourceName, generatedName] of generatedFiles) {
  const sourcePath = path.join(repoRoot, sourceName);
  const generatedPath = path.join(packageRoot, generatedName);

  if (
    existsSync(generatedPath) &&
    existsSync(sourcePath) &&
    readFileSync(generatedPath, "utf8") === readFileSync(sourcePath, "utf8")
  ) {
    unlinkSync(generatedPath);
  }
}
