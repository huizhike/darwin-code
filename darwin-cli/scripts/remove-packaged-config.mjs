#!/usr/bin/env node
import { existsSync, readFileSync, unlinkSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const packageRoot = path.resolve(__dirname, "..");
const repoRoot = path.resolve(packageRoot, "..");
const generatedConfig = path.join(packageRoot, "config.toml");

if (existsSync(generatedConfig)) {
  const contents = readFileSync(generatedConfig, "utf8");
  if (
    contents.includes("YOUR_QWEN_API_KEY") ||
    contents.includes("YOUR_PROVIDER_API_KEY")
  ) {
    unlinkSync(generatedConfig);
  }
}

for (const generatedName of ["LICENSE", "NOTICE"]) {
  const generatedPath = path.join(packageRoot, generatedName);
  const sourcePath = path.join(repoRoot, generatedName);

  if (
    existsSync(generatedPath) &&
    existsSync(sourcePath) &&
    readFileSync(generatedPath, "utf8") === readFileSync(sourcePath, "utf8")
  ) {
    unlinkSync(generatedPath);
  }
}
