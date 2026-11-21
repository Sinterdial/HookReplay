<script setup lang="ts">
/**
 * 通用分栏宽度拖动手柄组件（SplitResizer）
 *
 * 特性：
 * 1. 基于 Pointer Events 与 setPointerCapture，拖动过程平滑且不惧指针滑入 textarea；
 * 2. 8px 隐形触发热区 + 居中 2px/3px Gemini 流光视觉指示线；
 * 3. 支持固定分栏与悬浮抽屉边缘两种定位模式，双击即可恢复默认宽度。
 */
import { onBeforeUnmount, ref } from "vue";

const props = withDefaults(
  defineProps<{
    active?: boolean;
    floating?: boolean;
    side?: "left" | "right";
  }>(),
  {
    active: false,
    floating: false,
    side: "right",
  }
);

const emit = defineEmits<{
  (e: "resize-start"): void;
  (e: "resize-move", deltaX: number): void;
  (e: "resize-end"): void;
  (e: "reset"): void;
}>();

const isDragging = ref(false);
let startX = 0;
let activePointerId: number | null = null;
let capturedEl: HTMLElement | null = null;

function handlePointerDown(ev: PointerEvent): void {
  if (ev.button !== 0) return;
  ev.preventDefault();
  ev.stopPropagation();
  const el = ev.currentTarget as HTMLElement;
  activePointerId = ev.pointerId;
  capturedEl = el;
  try {
    el.setPointerCapture(ev.pointerId);
  } catch {
    // 忽略测试或非标准环境捕获异常
  }
  startX = ev.clientX;
  isDragging.value = true;
  emit("resize-start");
}

function handlePointerMove(ev: PointerEvent): void {
  if (!isDragging.value) return;
  emit("resize-move", ev.clientX - startX);
}

function finishDrag(ev?: PointerEvent): void {
  if (!isDragging.value) return;
  isDragging.value = false;
  if (ev && capturedEl && activePointerId !== null) {
    try {
      if (capturedEl.hasPointerCapture(activePointerId)) {
        capturedEl.releasePointerCapture(activePointerId);
      }
    } catch {
      // 忽略
    }
  }
  activePointerId = null;
  capturedEl = null;
  emit("resize-end");
}

onBeforeUnmount(() => {
  if (isDragging.value) {
    isDragging.value = false;
    emit("resize-end");
  }
});
</script>

<template>
  <div
    role="separator"
    aria-orientation="vertical"
    tabindex="-1"
    title="拖动调整分栏宽度，双击恢复默认宽度"
    @pointerdown="handlePointerDown"
    @pointermove="handlePointerMove"
    @pointerup="finishDrag"
    @pointercancel="finishDrag"
    @dblclick.stop="emit('reset')"
    class="group absolute inset-y-0 z-20 flex w-2.5 cursor-col-resize select-none items-center justify-center touch-none"
    :class="[
      props.floating
        ? props.side === 'left'
          ? 'left-0'
          : 'right-0'
        : props.side === 'left'
          ? '-left-1'
          : '-right-1',
    ]"
  >
    <div
      class="aurora-resizer-bar h-full transition-all duration-150"
      :class="[
        props.active || isDragging
          ? 'is-active w-[3px] opacity-100'
          : 'w-[2px] opacity-0 group-hover:w-[3px] group-hover:opacity-100',
      ]"
    />
  </div>
</template>
