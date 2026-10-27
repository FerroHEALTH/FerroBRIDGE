- **The operator console takes FerroUI 0.5.4** (`=0.5.4`, from 0.5.1). It
  hydrates through FerroUI's `widget::hydration::hydrate`, so the page's
  mark is `data-ferroui-hydrated` and a click or a submit before hydration
  ends is left to the browser; a post the console draws in the page counts
  as drawn, so a busy search box ends its wait; the mapping editor and the
  playground's input take the `_by_edit` editors and keep no copy of the
  text; the skip link moves the focus to the main area on a hydrated page.
  The forms keep their own post routes as the no-WebAssembly path until
  FerroUI's `<ActionForm>` can bring an answer back to its page.
