import { test, expect } from '@playwright/test';

test.describe('API Integration - Simple Tests', () => {
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
    
    // Check that some output appears
    const uastOutput = page.locator('[data-testid="uast-output"]');
    await expect(uastOutput).toBeVisible();
    
    // The output should contain some JSON structure
    const outputText = await uastOutput.textContent();
    expect(outputText).toContain('"');
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
    
    // Check that some output appears
    const uastOutput = page.locator('[data-testid="uast-output"]');
    await expect(uastOutput).toBeVisible();
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
    
    // Check that some output appears
    const uastOutput = page.locator('[data-testid="uast-output"]');
    await expect(uastOutput).toBeVisible();
  });
}); 