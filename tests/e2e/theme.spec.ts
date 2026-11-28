import { expect, test, type Page } from "@playwright/test";

async function savedAccent(page: Page) {
  return page.evaluate(async () => {
    const moduleUrl = new URL("/src/lib/api.ts", location.origin).href;
    const { api } = await import(moduleUrl) as typeof import("../../src/lib/api");
    return (await api.getLibrary()).settings.accent;
  });
}

test("主题色板按窗口宽度排版，预览和保存自定义颜色，并可恢复晴空蓝", async ({ page }) => {
  await page.goto("/#/library");
  await page.getByRole("button", { name: "设置", exact: true }).click();
  const settings = page.getByRole("dialog", { name: "设置" });
  const palette = settings.getByRole("group", { name: "主题强调色" });
  const choices = palette.getByRole("button");
  const sky = palette.getByRole("button", { name: "晴空蓝" });
  const hex = settings.getByRole("textbox", { name: "自定义 HEX 颜色" });
  const root = page.locator("html");
  const columns = () => palette.evaluate((grid) => getComputedStyle(grid).gridTemplateColumns.split(" ").length);

  await expect(root).toHaveAttribute("data-theme", "paper");
  await expect(sky).toHaveAttribute("aria-pressed", "true");
  await expect(choices).toHaveCount(9);
  await expect.poll(columns).toBe(5);
  expect(await choices.evaluateAll((buttons) => buttons.map((button) => (button as HTMLButtonElement).title.split(" · ")[1]))).toEqual([
    "#5E6AD2", "#4F46E5", "#7C3AED", "#0284C7", "#059669", "#F59E0B", "#EA580C", "#E11D48", "#52525B",
  ]);

  await page.setViewportSize({ width: 560, height: 640 });
  await expect.poll(columns).toBe(3);
  await expect(palette.getByRole("button", { name: "墨灰" })).toBeVisible();

  await hex.fill("#abc");
  await expect(root).toHaveCSS("--accent", "#aabbcc");
  expect(await savedAccent(page)).toBe("sky");
  await hex.fill("#abcdefg");
  await expect(hex).toHaveAttribute("aria-invalid", "true");
  await expect(root).toHaveCSS("--accent", "#0284c7");
  await hex.fill("#abcdef");
  await hex.press("Enter");
  await expect(hex).toHaveValue("#ABCDEF");
  await expect.poll(() => savedAccent(page)).toBe("#abcdef");

  const picker = settings.getByLabel("拾取自定义颜色", { exact: true });
  await picker.evaluate((input: HTMLInputElement) => {
    input.value = "#587567";
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await expect(hex).toHaveValue("#587567");
  await expect(root).toHaveCSS("--accent", "#587567");
  expect(await savedAccent(page)).toBe("#abcdef");
  await picker.dispatchEvent("change");
  await expect.poll(() => savedAccent(page)).toBe("#587567");

  await hex.fill("#bca");
  await settings.getByRole("button", { name: "恢复默认" }).click();
  await expect(sky).toHaveAttribute("aria-pressed", "true");
  await expect(hex).toHaveValue("#0284C7");
  await expect(root).toHaveCSS("--accent", "#0284c7");
  await expect.poll(() => savedAccent(page)).toBe("sky");
});
