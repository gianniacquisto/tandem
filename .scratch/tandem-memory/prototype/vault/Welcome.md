----
tags: [welcome]
created: 2026-10-16


# hi

hello

this wo
this is markdown is it

-[ ]
-
- 
[- 
-
-
-
[ ]
[]
[][]

## hello

# Welcome

This note exercises every rendered feature.

## Links

- Resolved bare: [[Ideas]]
- Resolved qualified: [[projects/Alpha]]
- Broken (ember, unobtrusive): [[broken-link]]
- Broken qualified (should create the folder): [[projects/Beta]]
- With alias: [[Ideas|the ideas note]]
- External: https://example.com

## Tasks

- [x] Click me to check
-  [ ]
-  [x] Click me to uncheck
- [x] Already done

## Math

Inline: $e^{i\pi} + 1 = 0$.

Display:

$$\int_{-\infty}^{\infty} e^{-x^2}\,dx = \sqrt{\pi}$$

## Code

```js
function greet(name) {
  return `hello, ${name}`;
}
```

```swift
print("unknown language stays plain monospace")
```

## Images

Real local: ![pixel](assets/pixel.png)

Missing local (broken-image placeholder): ![nope](assets/missing.png)

Remote (never fetched, placeholder): ![remote](https://example.com/photo.jpg)

## Backlinks

This note should appear in the backlinks dock of `Ideas.md` and `projects/Alpha.md`.
