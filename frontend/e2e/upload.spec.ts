import { test as base, expect, type APIRequestContext, type Page } from '@playwright/test'

type Settings = Record<string, unknown>
type FilePayload = { name: string; mimeType: string; buffer: Buffer }

const ADMIN = { login: 'admin', password: 'change-me-123' }

const token = (request: APIRequestContext): Promise<string> =>
  request
    .post('/api/auth/login', { data: ADMIN })
    .then((response) => response.json())
    .then((body) => body.token)

// `settings(patch)` edits the global upload settings for one test, they are
// restored afterwards. Call it before `login`, which caches the public ones.
const test = base.extend<{ settings: (patch: Settings) => Promise<void> }>({
  settings: async ({ request }, use) => {
    const headers = { authorization: `Bearer ${await token(request)}` }
    const original: Settings = await (await request.get('/api/elysia-settings', { headers })).json()
    const put = async (data: Settings) => expect(await request.put('/api/elysia-settings', { headers, data })).toBeOK()
    await use((patch) => put({ ...original, ...patch }))
    await put(original)
  },
})

const login = async (page: Page) => {
  await page.goto('/login')
  await page.getByPlaceholder('Username or email').fill(ADMIN.login)
  await page.getByPlaceholder('Password').fill(ADMIN.password)
  await page.getByRole('button', { name: 'Login', exact: true }).click()
  await expect(page).toHaveURL('/upload')
}

// A canvas of random pixels, so every image is new to the deduplication.
const image = async (page: Page, name: string, width = 200, height = 100): Promise<FilePayload> => {
  const mimeType = { png: 'image/png', jpg: 'image/jpeg', webp: 'image/webp' }[name.split('.').pop() ?? ''] ?? 'image/png'
  const base64 = await page.evaluate(
    ([mimeType, width, height]) => {
      const canvas = document.createElement('canvas')
      canvas.width = width
      canvas.height = height
      const context = canvas.getContext('2d')!
      const pixels = context.createImageData(width, height)
      pixels.data.forEach((_, i) => (pixels.data[i] = i % 4 === 3 ? 255 : Math.random() * 256))
      context.putImageData(pixels, 0, 0)
      return canvas.toDataURL(mimeType).split(',')[1]
    },
    [mimeType, width, height] as const,
  )
  return { name, mimeType, buffer: Buffer.from(base64, 'base64') }
}

// A 1x1 GIF of a random color (canvas cannot encode GIF).
const gif = (name: string): FilePayload => {
  const buffer = Buffer.from('R0lGODlhAQABAIAAAP///wAAACH5BAEAAAAALAAAAAABAAEAAAICRAEAOw==', 'base64')
  crypto.getRandomValues(buffer.subarray(13, 16))
  return { name, mimeType: 'image/gif', buffer }
}

const choose = (page: Page, files: FilePayload[]) => page.locator('input[type=file]').setInputFiles(files)
const uploadButton = (page: Page) => page.getByRole('button', { name: 'Upload', exact: true })
const toast = (page: Page, text: string | RegExp) => page.locator('.p-toast-message').filter({ hasText: text })

// Uploads what is selected and returns the ids listed on the result page.
const upload = async (page: Page, count: number): Promise<string[]> => {
  await uploadButton(page).click()
  await expect(page).toHaveURL(/\/uploaded\?/)
  const links = page.locator('a[href^="/i/"]')
  await expect(links).toHaveCount(count)
  return (await links.evaluateAll((anchors) => anchors.map((a) => a.getAttribute('href')!))).map((href) => href.slice(3))
}

// The value of a copyable link row, `group` 0 for the full image, 1 for the thumbnail.
const linkRow = (page: Page, label: string, group = 0) => page.locator('.p-inputgroup').filter({ hasText: label }).nth(group).locator('input, textarea')

// The local date `days` after any of `instants`, as the viewer shows it: an
// upload can straddle midnight, so each candidate day is accepted.
const daysAfter = (days: number, ...instants: number[]) =>
  new RegExp(instants.map((instant) => new Date(instant + days * 86_400_000).toLocaleString('sv-SE').slice(0, 10)).join('|'))

const metaValue = (page: Page, label: string) => page.locator('dt', { hasText: label }).locator('xpath=following-sibling::dd[1]')

// Opens the viewer of an image and checks it is served.
const view = async (page: Page, id: string) => {
  await page.goto(`/i/${id}`)
  const img = page.locator('.image-wrap > img')
  await expect(img).toBeVisible()
  expect(await img.evaluate((element: HTMLImageElement) => element.complete && element.naturalWidth > 0)).toBe(true)
}

test.describe('login', () => {
  test('pages need a session', async ({ page }) => {
    await page.goto('/upload')
    await expect(page).toHaveURL('/login')
  })

  test('wrong password shows an error', async ({ page }) => {
    await page.goto('/login')
    await page.getByPlaceholder('Username or email').fill(ADMIN.login)
    await page.getByPlaceholder('Password').fill('wrong-password')
    await page.getByRole('button', { name: 'Login', exact: true }).click()
    await expect(page.getByText('Wrong username or password.')).toBeVisible()
    await expect(page).toHaveURL('/login')
  })
})

test.describe('upload', () => {
  test.beforeEach(async ({ page }) => login(page))

  test('single PNG is converted with the default settings and viewable', async ({ page, context, request }) => {
    await choose(page, [await image(page, 'single.png')])
    await expect(page.locator('.preview')).toHaveCount(1)
    const [id] = await upload(page, 1)

    const link = await linkRow(page, 'Link').inputValue()
    expect(link).toMatch(new RegExp(`/i/${id}\\.webp$`))
    await expect(linkRow(page, 'Markdown')).toHaveValue(`![](${link})`)
    await expect(linkRow(page, 'BBCode')).toHaveValue(`[img]${link}[/img]`)
    await expect(linkRow(page, 'HTML')).toHaveValue(`<img src="${link}">`)
    const thumbnail = await linkRow(page, 'Link', 1).inputValue()
    await expect(linkRow(page, 'BBCode', 1)).toHaveValue(`[url=${link}][img]${thumbnail}[/img][/url]`)

    await page.locator('.p-inputgroup').filter({ hasText: 'Markdown' }).first().getByRole('button', { name: 'Copy' }).click()
    await expect(toast(page, 'Copied')).toBeVisible()
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(`![](${link})`)

    const served = await request.get(new URL(link).pathname)
    expect(served.headers()['content-type']).toBe('image/webp')
    expect((await request.get(new URL(thumbnail).pathname)).ok()).toBe(true)

    // The preview opens the viewer in a new tab.
    const [viewer] = await Promise.all([context.waitForEvent('page'), page.locator(`a[href="/i/${id}"]`).click()])
    await expect(viewer).toHaveURL(`/i/${id}`)
    await expect(metaValue(viewer, 'Name')).toHaveText('single.png')
    await expect(metaValue(viewer, 'Uploaded by')).toHaveText('admin')
    await expect(metaValue(viewer, 'Dimensions')).toHaveText('200 × 100')
    await expect(metaValue(viewer, 'Format')).toHaveText('image/webp')
    await expect(metaValue(viewer, 'Processing')).toContainText('Converted from PNG to WEBP')
    await expect(metaValue(viewer, 'Processing')).toContainText('Compressed at quality 75')
    await expect(viewer.locator('dt', { hasText: 'Expires at' })).toHaveCount(0)
    await view(viewer, id)
  })

  test('several formats at once keep their order', async ({ page }) => {
    const files = [await image(page, 'a.png'), await image(page, 'b.jpg'), await image(page, 'c.webp'), gif('d.gif')]
    await choose(page, files)
    await expect(page.locator('.preview')).toHaveCount(4)
    const ids = await upload(page, 4)
    expect(new Set(ids).size).toBe(4)

    const lines = (await linkRow(page, 'Link').inputValue()).split('\n')
    expect(lines).toHaveLength(4)
    lines.forEach((line, i) => expect(line).toContain(`/i/${ids[i]}.`))
    expect((await linkRow(page, 'Markdown').inputValue()).split('\n')).toHaveLength(4)

    for (const [i, id] of ids.entries()) {
      await view(page, id)
      await expect(metaValue(page, 'Name')).toHaveText(files[i].name)
    }
  })

  test('a removed image is not uploaded', async ({ page }) => {
    await choose(page, [await image(page, 'kept.png'), await image(page, 'removed.png')])
    await expect(page.locator('.preview')).toHaveCount(2)
    await page.getByRole('button', { name: 'Remove image' }).nth(1).click()
    await expect(page.locator('.preview')).toHaveCount(1)
    const [id] = await upload(page, 1)
    await view(page, id)
    await expect(metaValue(page, 'Name')).toHaveText('kept.png')
  })

  test('removing every image re-enables the URL field', async ({ page }) => {
    await expect(uploadButton(page)).toBeDisabled()
    await choose(page, [await image(page, 'gone.png')])
    await expect(page.getByPlaceholder('Paste an image link to rehost')).toBeHidden()
    await expect(uploadButton(page)).toBeEnabled()
    await page.getByRole('button', { name: 'Remove image' }).click()
    await expect(page.getByPlaceholder('Paste an image link to rehost')).toBeVisible()
    await expect(uploadButton(page)).toBeDisabled()
  })

  test('picking files twice adds to the selection', async ({ page }) => {
    await choose(page, [await image(page, 'first.png')])
    await choose(page, [await image(page, 'second.png')])
    await expect(page.locator('.preview')).toHaveCount(2)
    await upload(page, 2)
  })

  test('drag and drop ignores non images', async ({ page }) => {
    const files = [await image(page, 'dropped.png'), { name: 'notes.txt', mimeType: 'text/plain', buffer: Buffer.from('hello') }]
    await page.evaluate(
      (files) => {
        const transfer = new DataTransfer()
        files.forEach(({ name, mimeType, base64 }) =>
          transfer.items.add(new File([Uint8Array.from(atob(base64), (c) => c.charCodeAt(0))], name, { type: mimeType })),
        )
        window.dispatchEvent(new DragEvent('dragover', { dataTransfer: transfer, cancelable: true }))
        window.dispatchEvent(new DragEvent('drop', { dataTransfer: transfer, cancelable: true }))
      },
      files.map(({ name, mimeType, buffer }) => ({ name, mimeType, base64: buffer.toString('base64') })),
    )
    await expect(page.locator('.preview')).toHaveCount(1)
    const [id] = await upload(page, 1)
    await view(page, id)
    await expect(metaValue(page, 'Name')).toHaveText('dropped.png')
  })

  test('pasted image is picked', async ({ page }) => {
    const { buffer } = await image(page, 'pasted.png')
    await page.evaluate((base64) => {
      const transfer = new DataTransfer()
      transfer.items.add(new File([Uint8Array.from(atob(base64), (c) => c.charCodeAt(0))], 'pasted.png', { type: 'image/png' }))
      window.dispatchEvent(new ClipboardEvent('paste', { clipboardData: transfer, cancelable: true }))
    }, buffer.toString('base64'))
    await expect(page.locator('.preview')).toHaveCount(1)
    const [id] = await upload(page, 1)
    await view(page, id)
    await expect(metaValue(page, 'Name')).toHaveText('pasted.png')
  })

  test('same file twice is deduplicated', async ({ page }) => {
    const file = await image(page, 'twice.png')
    await choose(page, [file])
    const [first] = await upload(page, 1)
    await page.getByRole('button', { name: 'Upload more' }).click()
    await expect(page).toHaveURL('/upload')
    await choose(page, [file])
    const [second] = await upload(page, 1)
    expect(second).toBe(first)
  })

  test('a file that is not an image is refused', async ({ page }) => {
    await choose(page, [{ name: 'fake.png', mimeType: 'image/png', buffer: Buffer.from('not an image at all') }])
    await uploadButton(page).click()
    await expect(toast(page, 'unsupported media type')).toBeVisible()
    await expect(page).toHaveURL('/upload')
    await expect(page.locator('.preview')).toHaveCount(0)
  })

  test('a failed file in a batch does not stop the others', async ({ page }) => {
    await choose(page, [
      await image(page, 'good-1.png'),
      { name: 'bad.png', mimeType: 'image/png', buffer: Buffer.from('broken') },
      await image(page, 'good-2.jpg'),
    ])
    const ids = await upload(page, 2)
    await expect(toast(page, 'unsupported media type')).toBeVisible()
    await view(page, ids[1])
    await expect(metaValue(page, 'Name')).toHaveText('good-2.jpg')
  })

  test('upload into an album', async ({ page, request }) => {
    const name = `e2e album ${Date.now()}`
    const headers = { authorization: `Bearer ${await token(request)}` }
    expect(await request.post('/api/albums', { headers, data: { name } })).toBeOK()
    await page.reload()

    await page.locator('.p-select', { hasText: 'No album' }).click()
    await page.getByRole('option', { name }).click()
    await choose(page, [await image(page, 'in-album.png')])
    const [id] = await upload(page, 1)
    await view(page, id)
    await expect(page.locator('.albums').getByRole('link', { name })).toBeVisible()
  })

  test('upload into an album created from the upload form', async ({ page }) => {
    const name = `e2e new album ${Date.now()}`
    await page.locator('.p-select', { hasText: 'No album' }).click()
    await page.getByRole('button', { name: 'New album' }).click()
    await page.getByLabel('Album name').fill(name)
    await page.getByRole('button', { name: 'Create' }).click()
    await expect(page.locator('.p-select', { hasText: name })).toBeVisible()
    await choose(page, [await image(page, 'in-new-album.png')])
    const [id] = await upload(page, 1)
    await view(page, id)
    await expect(page.locator('.albums').getByRole('link', { name })).toBeVisible()
  })
})

test.describe('upload settings', () => {
  test('processing off stores the file as uploaded', async ({ page, settings }) => {
    await settings({ default_target_file_format: null, default_compression: 0, allow_overriding_file_format: false, allow_overriding_compression: false })
    await login(page)
    await choose(page, [await image(page, 'raw.png')])
    const [id] = await upload(page, 1)
    await expect(linkRow(page, 'Link')).toHaveValue(new RegExp(`/i/${id}\\.png$`))
    await view(page, id)
    await expect(metaValue(page, 'Format')).toHaveText('image/png')
    await expect(metaValue(page, 'Processing')).toHaveText('None, stored as uploaded')
  })

  test('large images are resized', async ({ page, settings }) => {
    await settings({ target_width_pixels: 50, target_height_pixels: 50 })
    await login(page)
    await choose(page, [await image(page, 'big.png', 200, 100)])
    const [id] = await upload(page, 1)
    await view(page, id)
    await expect(metaValue(page, 'Dimensions')).toHaveText('50 × 25')
    await expect(metaValue(page, 'Processing')).toContainText('Resized from 200 × 100')
  })

  test('file size limit is checked before uploading', async ({ page, settings }) => {
    await settings({ max_file_size_bytes: 1000 })
    await login(page)
    await choose(page, [await image(page, 'heavy.png')])
    await expect(toast(page, 'heavy.png: Image is too large (max')).toBeVisible()
    await expect(page.locator('.preview')).toHaveCount(0)
    await expect(uploadButton(page)).toBeDisabled()
  })

  test('dimension limits are checked before uploading', async ({ page, settings }) => {
    await settings({ max_width_pixels: 150, max_height_pixels: 150 })
    await login(page)
    await choose(page, [await image(page, 'wide.png', 200, 100), await image(page, 'tall.png', 100, 200), await image(page, 'fits.png', 150, 150)])
    await expect(toast(page, 'wide.png: Image is too wide (max 150px)')).toBeVisible()
    await expect(toast(page, 'tall.png: Image is too tall (max 150px)')).toBeVisible()
    await expect(page.locator('.preview')).toHaveCount(1)
    const [id] = await upload(page, 1)
    await view(page, id)
    await expect(metaValue(page, 'Name')).toHaveText('fits.png')
  })

  test('format and quality overrides', async ({ page, settings }) => {
    await settings({ allow_overriding_file_format: true, allow_overriding_compression: true })
    await login(page)
    const quality = page.getByText(/^Compression quality: \d+$/)
    await expect(quality).toHaveText('Compression quality: 80')

    await page.locator('.p-select', { hasText: 'Use default' }).click()
    await page.getByRole('option', { name: 'AVIF' }).click()
    await expect(quality).toBeVisible()
    await expect(page.locator('.toggle-row', { hasText: 'Lossless' })).toHaveCount(0)
    await page.locator('.p-slider-handle').focus()
    await page.keyboard.press('ArrowLeft')
    await expect(quality).toHaveText('Compression quality: 79')
    await choose(page, [await image(page, 'to-avif.jpg')])
    const [avif] = await upload(page, 1)
    await expect(linkRow(page, 'Link')).toHaveValue(new RegExp(`/i/${avif}\\.avif$`))
    await view(page, avif)
    await expect(metaValue(page, 'Format')).toHaveText('image/avif')
    await expect(metaValue(page, 'Processing')).toContainText('Converted from JPG to AVIF')
    await expect(metaValue(page, 'Processing')).toContainText('Compressed at quality 79')

    // PNG is always lossless: no quality slider.
    await page.goto('/upload')
    await page.locator('.p-select', { hasText: 'Use default' }).click()
    await page.getByRole('option', { name: 'PNG' }).click()
    await expect(quality).toBeHidden()
    await choose(page, [await image(page, 'to-png.jpg')])
    const [png] = await upload(page, 1)
    await view(page, png)
    await expect(metaValue(page, 'Format')).toHaveText('image/png')
    await expect(metaValue(page, 'Processing')).toHaveText('Converted from JPG to PNG')
  })

  test('lossless WebP', async ({ page, settings }) => {
    await settings({ allow_overriding_compression: true })
    await login(page)
    await page.locator('.toggle-row', { hasText: 'Lossless' }).getByRole('switch').check()
    await expect(page.getByText(/^Compression quality/)).toBeHidden()
    await choose(page, [await image(page, 'lossless.png')])
    const [id] = await upload(page, 1)
    await view(page, id)
    await expect(metaValue(page, 'Format')).toHaveText('image/webp')
    await expect(metaValue(page, 'Processing')).toHaveText('Converted from PNG to WEBP')
  })

  test('format and quality options hidden when not overridable', async ({ page, settings }) => {
    await settings({
      allow_overriding_file_format: false,
      allow_overriding_compression: false,
      allow_overriding_strip_exif: false,
      allow_overriding_expiry: false,
    })
    await login(page)
    await expect(page.getByText('Output format')).toBeHidden()
    await expect(page.getByText(/^Compression quality/)).toBeHidden()
    await expect(page.getByText('Lossless')).toBeHidden()
    await expect(page.getByText('Strip EXIF')).toBeHidden()
    await expect(page.getByText('Delete after')).toBeHidden()
  })

  test('expiry override', async ({ page, settings }) => {
    await settings({ allow_overriding_expiry: true, default_expiry_days: 7 })
    await login(page)
    const days = page.locator('.format-row', { hasText: 'Delete after' }).locator('input')
    await expect(days).toHaveValue('7')
    await days.fill('3')
    await choose(page, [await image(page, 'expiring.png')])
    const start = Date.now()
    const [id] = await upload(page, 1)
    await view(page, id)
    await expect(metaValue(page, 'Expires at')).toContainText(daysAfter(3, start, Date.now()))

    await page.goto('/upload')
    await days.fill('0')
    await choose(page, [await image(page, 'forever.png')])
    const [forever] = await upload(page, 1)
    await view(page, forever)
    await expect(page.locator('dt', { hasText: 'Expires at' })).toHaveCount(0)
  })

  test('default expiry applies when not overridable', async ({ page, settings }) => {
    await settings({ allow_overriding_expiry: false, default_expiry_days: 2 })
    await login(page)
    await choose(page, [await image(page, 'default-expiry.png')])
    const start = Date.now()
    const [id] = await upload(page, 1)
    await view(page, id)
    await expect(metaValue(page, 'Expires at')).toContainText(daysAfter(2, start, Date.now()))
  })

  test('uploads disabled', async ({ page, settings }) => {
    await settings({ uploads_enabled: false })
    await login(page)
    await expect(page.getByText('Uploads are temporarily disabled.')).toBeVisible()
    await expect(page.locator('input[type=file]')).toHaveCount(0)
    await expect(uploadButton(page)).toHaveCount(0)
  })
})

test.describe('rehost from a URL', () => {
  test.beforeEach(async ({ page }) => login(page))

  const urlField = (page: Page) => page.getByPlaceholder('Paste an image link to rehost')

  test('an invalid URL is refused client side', async ({ page }) => {
    await urlField(page).fill('not a url')
    await uploadButton(page).click()
    await expect(toast(page, 'Please enter a valid URL')).toBeVisible()
    await expect(page).toHaveURL('/upload')
  })

  test('a private address is refused', async ({ page }) => {
    await urlField(page).fill('http://127.0.0.1:8091/i/missing.png')
    await urlField(page).press('Enter')
    await expect(page.locator('.p-toast-message-error')).toBeVisible()
    await expect(page).toHaveURL('/upload')
  })

  test('a public image is rehosted', async ({ page }) => {
    test.skip(!!process.env.CI, 'needs internet access, local only')
    await urlField(page).fill('https://www.google.com/images/branding/googlelogo/1x/googlelogo_color_272x92dp.png')
    const [id] = await upload(page, 1)
    await view(page, id)
    await expect(metaValue(page, 'Dimensions')).toHaveText('272 × 92')
    await expect(metaValue(page, 'Processing')).toContainText('Converted from PNG to WEBP')
  })

  test('picking a file hides the URL field', async ({ page }) => {
    await urlField(page).fill('https://example.com/a.png')
    await expect(page.locator('input[type=file]')).toHaveCount(0)
    await urlField(page).fill('')
    await choose(page, [await image(page, 'hides-url.png')])
    await expect(urlField(page)).toBeHidden()
  })
})
