#!/usr/bin/env node
/**
 * Captures live, populated screenshots from the running containerised Magpie Gateway
 * (http://127.0.0.1:8088) for Josh's Client Demo Guide.
 */
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { chromium } = require("../../magpie/axe/node_modules/playwright");

const BASE = process.env.MAGPIE_PORTAL_URL || "http://127.0.0.1:8088";
const PASSWORD = process.env.MAGPIE_ADMIN_PASSWORD || "MagpieDemoAdmin2026!";
const OUT_DIR = path.resolve("docs/screenshots/demo");
fs.mkdirSync(OUT_DIR, { recursive: true });

async function main() {
  const browser = await chromium.launch({
    executablePath: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  });
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    deviceScaleFactor: 2,
  });
  const page = await context.newPage();

  // 1. Login screen
  await page.goto(`${BASE}/ui/login`, { waitUntil: "networkidle" });
  await page.waitForTimeout(500);
  await page.screenshot({ path: path.join(OUT_DIR, "demo-01-login.png") });
  console.log("Captured demo-01-login.png");

  // Sign in via API inside browser context so cookie is set cleanly
  const loginOk = await page.evaluate(async (pwd) => {
    const res = await fetch("/api/magpie/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: "admin", password: pwd }),
    });
    return res.ok;
  }, PASSWORD);
  if (!loginOk) throw new Error("Portal login failed in screenshot script");

  const shots = [
    ["demo-02-control-centre.png", "/ui/control-centre", false],
    ["demo-03-approvals-inbox.png", "/ui/approvals", false],
    ["demo-04-audit-live-blocks.png", "/audit", false],
    ["demo-05-dlp-australia.png", "/ui/dlp-tuning", false],
    ["demo-06-identity-scim.png", "/ui/identity", false],
    ["demo-07-client-onboarding.png", "/ui/llm/client-setup", false],
    ["demo-08-siem-live-sink.png", "/ui/siem", false],
    ["demo-09-evidence-live-pack.png", "/ui/evidence", false],
    ["demo-10-native-gateway.png", "/ui/", false],
  ];

  for (const [file, route, fullPage] of shots) {
    await page.goto(`${BASE}${route}`, { waitUntil: "networkidle" });
    await page.waitForTimeout(700);
    if (file === "demo-04-audit-live-blocks.png") {
      // Filter live audit log inside the embedded frame to show real agent blocked events
      const frame = page.frameLocator("iframe").first();
      await frame.locator('input[name="decision"]').fill("block", { timeout: 5000 }).catch(() => {});
      await frame.locator('button:has-text("Filter")').click({ timeout: 5000 }).catch(() => {});
      await page.waitForTimeout(900);
    }
    await page.screenshot({ path: path.join(OUT_DIR, file), fullPage });
    console.log(`Captured ${file} (${route})`);
  }

  await browser.close();
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
