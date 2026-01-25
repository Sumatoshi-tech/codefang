import { test, expect } from '@playwright/test';

test.describe('UAST Development Service', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('should load the main page with correct title', async ({ page }) => {
    await expect(page).toHaveTitle('UAST Mapping Development Service');
    
    // Check that main sections exist
    await expect(page.locator('text=UAST Mapping Editor')).toBeVisible();
    await expect(page.locator('text=Code & Output')).toBeVisible();
  });

  test('should display main layout components', async ({ page }) => {
    // Check that main sections exist
    await expect(page.locator('text=UAST Mapping Editor')).toBeVisible();
    await expect(page.locator('text=Code & Output')).toBeVisible();
  });

  test('should have language selector with Go as default', async ({ page }) => {
    const languageSelect = page.locator('[data-testid="language-selector"]');
    await expect(languageSelect).toBeVisible();
    
    // Check that Go is selected by default
    await expect(languageSelect).toContainText('Go');
  });

  test('should have code editor textarea', async ({ page }) => {
    const codeTextarea = page.locator('[data-testid="code-editor"]');
    await expect(codeTextarea).toBeVisible();
    
    // Code editor starts empty
    await expect(codeTextarea).toHaveValue('');
  });

  test('should have query input field', async ({ page }) => {
    const queryInput = page.locator('[data-testid="query-input"]');
    await expect(queryInput).toBeVisible();
    await expect(queryInput).toBeEnabled();
  });

  test('should display examples button in header', async ({ page }) => {
    // Check that examples button exists in header
    await expect(page.locator('button:has-text("Examples")')).toBeVisible();
  });

  test('should show examples panel when clicking examples button', async ({ page }) => {
    // Click on examples button
    await page.locator('button:has-text("Examples")').click();

    // Check examples panel is visible
    await expect(page.locator('text=Example Mappings')).toBeVisible();
    
    // Check for example mapping buttons using button selectors
    await expect(page.locator('button:has-text("Empty Custom Mapping")')).toBeVisible();
    await expect(page.locator('button:has-text("Basic Function Mapping")')).toBeVisible();
    await expect(page.locator('button:has-text("Variable Declaration Mapping")')).toBeVisible();
  });

  test('should change language when selected', async ({ page }) => {
    // Wait for page to load
    await page.waitForTimeout(2000);
    
    // Click on language selector
    await page.locator('[data-testid="language-selector"]').click();
    
    // Select Python
    await page.locator('[data-testid="language-option-python"]').click();
    
    // Check that Python is selected
    await expect(page.locator('[data-testid="language-selector"]')).toContainText('Python');
  });

  test('should show loading state when parsing code', async ({ page }) => {
    // Wait for page to load and mapping to be selected
    await page.waitForTimeout(2000);
    
    // Check that parsing badge appears when code is being parsed
    // This happens automatically when code is entered and mapping is selected
    const codeTextarea = page.locator('[data-testid="code-editor"]');
    await codeTextarea.fill('package main\n\nfunc main() {\n    println("Hello")\n}');
    
    // Should show parsing badge (be more specific to avoid multiple matches)
    await expect(page.locator('text=Parse').first()).toBeVisible({ timeout: 5000 });
  });

  test('should show loading state when executing query', async ({ page }) => {
    // Wait for page to load and mapping to be selected
    await page.waitForTimeout(2000);
    
    // Enter some code first
    const codeTextarea = page.locator('[data-testid="code-editor"]');
    await codeTextarea.fill('package main\n\nfunc main() {\n    println("Hello")\n}');
    
    // Wait for parsing to complete
    await expect(page.locator('text=Parse').first()).toBeVisible({ timeout: 10000 });
    await page.waitForTimeout(3000);
    
    // Enter a query
    const queryInput = page.locator('[data-testid="query-input"]');
    await queryInput.fill('filter(.type == "Test")');
    
    // Should show querying badge
    await expect(page.locator('text=Querying...')).toBeVisible({ timeout: 5000 });
  });

  test('should create custom mapping when example is clicked', async ({ page }) => {
    // Click on examples button
    await page.locator('button:has-text("Examples")').click();
    
    // Click on first example
    await page.locator('button:has-text("Empty Custom Mapping")').click();
    
    // Check that a custom mapping was created and selected
    await expect(page.locator('[data-testid="mapping-option-empty_custom_mapping"]')).toBeVisible();
  });

  test('should execute query on Enter key', async ({ page }) => {
    // Wait for page to load and mapping to be selected
    await page.waitForTimeout(2000);
    
    // Enter some code first
    const codeTextarea = page.locator('[data-testid="code-editor"]');
    await codeTextarea.fill('package main\n\nfunc main() {\n    println("Hello")\n}');
    
    // Wait for parsing to complete
    await expect(page.locator('text=Parse').first()).toBeVisible({ timeout: 10000 });
    await page.waitForTimeout(3000);
    
    // Enter a query and press Enter
    const queryInput = page.locator('[data-testid="query-input"]');
    await queryInput.fill('filter(.type == "Test")');
    await queryInput.press('Enter');
    
    // Wait for query to complete and check output
    await page.waitForTimeout(3000);
    const uastOutput = page.locator('[data-testid="uast-output"]');
    await expect(uastOutput).toBeVisible();
  });

  test('should be responsive on mobile viewport', async ({ page }) => {
    // Set mobile viewport
    await page.setViewportSize({ width: 375, height: 667 });
    
    // Check that main sections are still visible
    await expect(page.locator('text=UAST Mapping Editor')).toBeVisible();
    await expect(page.locator('text=Code & Output')).toBeVisible();
    
    // Check that examples button is still visible
    await expect(page.locator('button:has-text("Examples")')).toBeVisible();
  });
}); 