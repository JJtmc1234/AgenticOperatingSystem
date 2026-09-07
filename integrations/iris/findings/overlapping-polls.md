# Room displays duplicate messages when poll responses overlap

Iris Playwright finding with independent source review. Reproduced twice in Chromium against the actual portal page, HTTP handlers and temporary SQLite through a local D1 adapter. This is not a Cloudflare deployment test.

Inspected commit: `20297ec61ae0717201d485d1fc0ce0820dc01221`.

### Room displays duplicate messages when poll responses overlap

One stored message appears twice in the chat transcript under a delayed network response.

Source: https://github.com/JJtmc1234/AgenticOperatingSystem/blob/20297ec61ae0717201d485d1fc0ce0820dc01221/carl/portal/page.js#L208

```
      for (var i = 0; i < said.length; i++) { show(said[i]); seen = said[i].id; }
```

poll() allows concurrent requests with the same seen watermark. Each response appends every returned message without checking its id against the current watermark. A delayed older response therefore renders messages already displayed by a newer response.

**Completion criteria and proposed test**

Keep the overlapping polls Playwright regression. With one stored message and the first read response delayed until the next interval finishes, exactly one message must remain visible. Preserve message order and normal cross-browser delivery.

<!-- aos-iris-finding:57e76e0adad72ea5e832855e -->

### Reproduction and observed result

1. Store one message through `/say`.
2. Sign in and hold the first `/read?after=0` response.
3. Allow the three-second interval to complete a second read and render the message.
4. Release the first response.
5. Observe two `.said` elements containing the same single stored message.

Playwright test: `overlapping polls display each message once`. Assertion: `expect(page.locator("#room .said")).toHaveCount(1)`. Actual count: `2`.

The corrected suite produced **5 passed, 1 failed, 0 skipped, 0 flaky** in 14.7 seconds. Other checks covered authentication, identity, cross-browser delivery, literal HTML, session persistence, sign-out and membership approval.

Local runner: `carl iris test`. Local evidence run: `20260907T210913973694Z`, with HTML report, failure screenshot and trace. The new suite is currently local rather than available on the remote branch.

<!-- aos-iris:0d7f8e56bfddd417f1c266de -->
