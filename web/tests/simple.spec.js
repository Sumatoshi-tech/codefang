import { test, expect } from '@playwright/test';

test('should load the main page', async ({ page }) => {
  await page.goto('/');

  // Check title
  await expect(page).toHaveTitle('UAST Mapping Development Service');

  // Check that main sections exist
  await expect(page.locator('text=UAST Mapping Editor')).toBeVisible();
  await expect(page.locator('text=Code & Output')).toBeVisible();
});

test('should have code editor and query input visible', async ({ page }) => {
  await page.goto('/');

  // Check code editor is visible
  await expect(page.locator('[data-testid="code-editor"]')).toBeVisible();

  // Check query input is visible
  await expect(page.locator('[data-testid="query-input"]')).toBeVisible();
});

test('should show examples panel when clicking examples button', async ({ page }) => {
  await page.goto('/');

  // Click on examples button
  await page.locator('button:has-text("Examples")').click();

  // Check examples panel is visible
  await expect(page.locator('text=Example Mappings')).toBeVisible();
}); 