import { test, expect } from '@playwright/test';

test.describe('API Integration Tests', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('should parse Go code successfully', async ({ page }) => {
    // Wait for the page to load and mapping to be selected
    await page.waitForTimeout(2000);
    
    // Enter some Go code
    const codeTextarea = page.locator('[data-testid="code-editor"]');
    await codeTextarea.fill('package main\n\nfunc main() {\n    println("Hello, World!")\n}');
    
    // Wait for parsing to complete (should happen automatically)
    await expect(page.locator('text=Parse').first()).toBeVisible({ timeout: 10000 });
    
    // Wait for parsing to finish
    await page.waitForTimeout(3000);
    
    // Check that UAST output contains some content
    const uastOutput = page.locator('[data-testid="uast-output"]');
    await expect(uastOutput).toBeVisible();
    
    // The output should contain UAST structure
    const outputText = await uastOutput.textContent();
    expect(outputText).toContain('"type"');
    expect(outputText).toContain('"children"');
  });

  test('should show error when parsing empty code', async ({ page }) => {
    // Wait for page to load
    await page.waitForTimeout(2000);
    
    // Clear the code editor
    await page.locator('[data-testid="code-editor"]').clear();
    
    // Wait a bit to see if any error appears
    await page.waitForTimeout(2000);
    
    // Check if any error message appears in the output
    const uastOutput = page.locator('[data-testid="uast-output"]');
    const outputText = await uastOutput.textContent();
    expect(outputText).toContain('No UAST data yet');
  });

  test('should execute query successfully', async ({ page }) => {
    // Wait for page to load and mapping to be selected
    await page.waitForTimeout(2000);
    
    // Enter some code first
    const codeTextarea = page.locator('[data-testid="code-editor"]');
    await codeTextarea.fill('package main\n\nfunc main() {\n    println("Hello, World!")\n}');
    
    // Wait for parsing to complete
    await expect(page.locator('text=Parse').first()).toBeVisible({ timeout: 10000 });
    await page.waitForTimeout(3000);
    
    // Enter a simple query
    const queryInput = page.locator('[data-testid="query-input"]');
    await queryInput.fill('filter(.type == "Package")');
    
    // Wait for query to complete
    await expect(page.locator('text=Querying...')).toBeVisible({ timeout: 10000 });
    await page.waitForTimeout(3000);
    
    // Check that query results contain some content
    const uastOutput = page.locator('[data-testid="uast-output"]');
    await expect(uastOutput).toBeVisible();
  });

  test('should show error when executing query without parsing first', async ({ page }) => {
    // Wait for page to load
    await page.waitForTimeout(2000);
    
    // Enter a query without entering code first
    const queryInput = page.locator('[data-testid="query-input"]');
    await queryInput.fill('filter(.type == "Test")');
    
    // Wait a bit for any error to appear
    await page.waitForTimeout(2000);
    
    // Check if error message appears in output
    const uastOutput = page.locator('[data-testid="uast-output"]');
    const outputText = await uastOutput.textContent();
    expect(outputText).toContain('No UAST data available');
  });

  test('should show UAST output when query is cleared', async ({ page }) => {
    // Wait for page to load and mapping to be selected
    await page.waitForTimeout(2000);
    
    // Enter some code first
    const codeTextarea = page.locator('[data-testid="code-editor"]');
    await codeTextarea.fill('package main\n\nfunc main() {\n    println("Hello, World!")\n}');
    
    // Wait for parsing to complete
    await expect(page.locator('text=Parse').first()).toBeVisible({ timeout: 10000 });
    await page.waitForTimeout(3000);
    
    // Enter a query first
    const queryInput = page.locator('[data-testid="query-input"]');
    await queryInput.fill('filter(.type == "Package")');
    await page.waitForTimeout(3000);
    
    // Clear query input
    await queryInput.clear();
    await page.waitForTimeout(2000);
    
    // Check that UAST output is shown instead of query results
    const uastOutput = page.locator('[data-testid="uast-output"]');
    const outputText = await uastOutput.textContent();
    expect(outputText).toContain('"type"');
    expect(outputText).toContain('"children"');
  });

  test('should change language and parse different code', async ({ page }) => {
    // Wait for page to load
    await page.waitForTimeout(2000);
    
    // Change language to Python
    await page.locator('[data-testid="language-selector"]').click();
    await page.locator('[data-testid="language-option-python"]').click();
    
    // Wait for language change to complete
    await page.waitForTimeout(2000);
    
    // Enter some Python code
    const codeTextarea = page.locator('[data-testid="code-editor"]');
    await codeTextarea.fill('def main():\n    print("Hello, World!")');
    
    // Parse the Python code (should happen automatically)
    await expect(page.locator('text=Parse').first()).toBeVisible({ timeout: 10000 });
    await page.waitForTimeout(3000);
    
    // Check that UAST output contains content
    const uastOutput = page.locator('[data-testid="uast-output"]');
    await expect(uastOutput).toBeVisible();
  });

  test('should create custom mapping when example is clicked', async ({ page }) => {
    // Wait for page to load and mapping to be selected
    await page.waitForTimeout(2000);
    
    // Enter some code first
    const codeTextarea = page.locator('[data-testid="code-editor"]');
    await codeTextarea.fill('package main\n\nfunc main() {\n    println("Hello, World!")\n}');
    
    // Wait for parsing to complete
    await expect(page.locator('text=Parse').first()).toBeVisible({ timeout: 10000 });
    await page.waitForTimeout(3000);
    
    // Click on examples button
    await page.locator('button:has-text("Examples")').click();
    
    // Click on an example
    await page.locator('button:has-text("Empty Custom Mapping")').click();
    
    // Check that a custom mapping was created and selected
    await expect(page.locator('[data-testid="mapping-option-empty_custom_mapping"]')).toBeVisible();
  });
}); 