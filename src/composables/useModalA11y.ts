import { onUnmounted, watch, type Ref } from "vue";

interface ModalA11yOptions {
  /** Visibility flag; may be a getter for prop-derived visibility. */
  visible: () => boolean;
  /** Ref bound to the dialog container element (only mounted while visible). */
  container: Ref<HTMLElement | null>;
  /** Called when Escape is pressed. Omit for modals that must not be dismissed. */
  onEscape?: () => void;
  /** Element to focus when the modal opens. Defaults to the first focusable. */
  initialFocus?: () => HTMLElement | null;
}

const FOCUSABLE_SELECTOR =
  'button:not(:disabled), [href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])';

function firstFocusable(root: HTMLElement): HTMLElement | null {
  const candidates = Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
  return candidates[0] ?? null;
}

/**
 * Shared modal accessibility baseline: focus is moved into the dialog on
 * open, cycled with Tab while open, and restored to the trigger on close.
 * Escape optionally triggers a dismiss callback. This exists so every modal
 * in the app behaves identically (see audit-17).
 */
export function useModalA11y({ visible, container, onEscape, initialFocus }: ModalA11yOptions) {
  let previousFocus: HTMLElement | null = null;

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      if (!onEscape) return;
      event.stopPropagation();
      onEscape();
      return;
    }
    if (event.key !== "Tab") return;

    const root = container.value;
    if (!root) return;
    const focusable = Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  watch(
    visible,
    (isVisible) => {
      if (isVisible) {
        previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
        // The dialog content is rendered in the same tick via v-if/Teleport;
        // wait one frame so querySelector sees the mounted subtree.
        requestAnimationFrame(() => {
          const root = container.value;
          if (!root) return;
          root.addEventListener("keydown", handleKeydown);
          const target = initialFocus?.() ?? firstFocusable(root);
          target?.focus();
        });
        return;
      }
      container.value?.removeEventListener("keydown", handleKeydown);
      if (previousFocus) {
        previousFocus.focus();
        previousFocus = null;
      }
    },
    { flush: "post" }
  );

  onUnmounted(() => {
    container.value?.removeEventListener("keydown", handleKeydown);
    previousFocus = null;
  });
}
