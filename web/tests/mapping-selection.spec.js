import { test, expect } from '@playwright/test'

test.describe('UAST Mapping Selection', () => {
  test.beforeEach(async ({ page }) => {
    // Navigate to the application
    await page.goto('http://localhost:3000')
    
    // Wait for the application to load
    await page.waitForSelector('[data-testid="language-selector"]', { timeout: 10000 })
  })

  test('should display language selector and allow language selection', async ({ page }) => {
    // Check that language selector is visible
    const languageSelector = page.locator('[data-testid="language-selector"]')
    await expect(languageSelector).toBeVisible()

    // Click to open dropdown
    await languageSelector.click()
    
    // Wait for dropdown to appear
    const dropdown = page.locator('[data-testid="language-dropdown"]')
    await expect(dropdown).toBeVisible()

    // Check that Go option is available
    const goOption = page.locator('[data-testid="language-option-go"]')
    await expect(goOption).toBeVisible()
  })

  test('should automatically load embedded mapping when language is selected', async ({ page }) => {
    // Select Go language
    const languageSelector = page.locator('[data-testid="language-selector"]')
    await languageSelector.click()
    
    const goOption = page.locator('[data-testid="language-option-go"]')
    await goOption.click()

    // Wait for the mapping editor to update
    await page.waitForTimeout(2000)

    // Check that the embedded mapping is automatically selected
    const embeddedMappingOption = page.locator('[data-testid="mapping-option-go_embedded"]')
    await expect(embeddedMappingOption).toBeVisible()
  })

  test('should display mapping content in editor when mapping is automatically selected', async ({ page }) => {
    // Select Go language
    const languageSelector = page.locator('[data-testid="language-selector"]')
    await languageSelector.click()
    
    const goOption = page.locator('[data-testid="language-option-go"]')
    await goOption.click()

    // Wait for the mapping editor to update
    await page.waitForTimeout(2000)

    // Check that mapping content is displayed
    const dslContent = page.locator('[data-testid="dsl-content"]')
    await expect(dslContent).toBeVisible()
    
    // Check that the content is not empty
    const content = await dslContent.inputValue()
    expect(content.length).toBeGreaterThan(0)
  })

  test('should allow creating and editing custom mappings', async ({ page }) => {
    // Click "New Custom" button
    const newCustomButton = page.locator('[data-testid="create-custom-button"]')
    await newCustomButton.click()

    // Wait for the new mapping to be created and selected
    await page.waitForTimeout(1000)

    // Check that the custom mapping is now selected
    const customMappingOption = page.locator('[data-testid="mapping-option-custom_mapping_1"]')
    await expect(customMappingOption).toBeVisible()

    // Check that the DSL content area is editable
    const dslContent = page.locator('[data-testid="dsl-content"]')
    await expect(dslContent).toBeVisible()
    
    // Verify it's not disabled (custom mappings should be editable)
    await expect(dslContent).not.toBeDisabled()
  })

  test('should parse code with selected mapping', async ({ page }) => {
    // Select Go language (embedded mapping will be automatically selected)
    const languageSelector = page.locator('[data-testid="language-selector"]')
    await languageSelector.click()
    
    const goOption = page.locator('[data-testid="language-option-go"]')
    await goOption.click()

    // Wait for the mapping editor to update
    await page.waitForTimeout(2000)

    // Enter some Go code
    const codeEditor = page.locator('[data-testid="code-editor"]')
    await codeEditor.fill('package main\n\nfunc main() {\n    println("Hello, World!")\n}')

    // Wait for parsing to complete
    await page.waitForTimeout(3000)

    // Check that UAST output is generated
    const uastOutput = page.locator('[data-testid="uast-output"]')
    await expect(uastOutput).toBeVisible()
    
    // Check that the output contains some content (not the default message)
    const outputText = await uastOutput.textContent()
    expect(outputText).not.toContain('No UAST data yet')
  })

  test('should handle multiple language selections', async ({ page }) => {
    // Select Go language first (embedded mapping will be automatically selected)
    const languageSelector = page.locator('[data-testid="language-selector"]')
    await languageSelector.click()
    
    const goOption = page.locator('[data-testid="language-option-go"]')
    await goOption.click()

    // Wait for the mapping editor to update
    await page.waitForTimeout(2000)

    // Check that the Go embedded mapping is selected
    const goEmbeddedOption = page.locator('[data-testid="mapping-option-go_embedded"]')
    await expect(goEmbeddedOption).toBeVisible()

    // Now select Python language
    await languageSelector.click()
    
    const pythonOption = page.locator('[data-testid="language-option-python"]')
    await pythonOption.click()

    // Wait for the mapping editor to update
    await page.waitForTimeout(2000)

    // Check that the Python embedded mapping is now selected
    const pythonEmbeddedOption = page.locator('[data-testid="mapping-option-python_embedded"]')
    await expect(pythonEmbeddedOption).toBeVisible()
  })

  test('should show "No UAST Mapping Selected" message when no mapping is selected', async ({ page }) => {
    // Clear any selected mapping by clicking "Clear Selection" if it exists
    const clearSelectionButton = page.locator('button:has-text("Clear Selection")')
    if (await clearSelectionButton.isVisible()) {
      await clearSelectionButton.click()
      await page.waitForTimeout(1000)
    }

    // Check that "No UAST Mapping Selected" message appears when no mapping is selected
    const noMappingMessage = page.locator('[data-testid="no-mapping-message"]')
    await expect(noMappingMessage).toBeVisible()
  })
}) 