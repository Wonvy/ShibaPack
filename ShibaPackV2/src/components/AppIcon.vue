<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { initial } from "../lib/format";

const props = withDefaults(
  defineProps<{ src?: string | null; name: string; size?: number; rounded?: string }>(),
  { src: null, size: 36, rounded: "10px" },
);

const failed = ref(false);
watch(
  () => props.src,
  () => {
    failed.value = false;
  },
);

const showImage = computed(() => Boolean(props.src) && !failed.value);
const boxStyle = computed(() => ({
  width: `${props.size}px`,
  height: `${props.size}px`,
  borderRadius: props.rounded,
}));
const fontSize = computed(() => `${Math.round(props.size * 0.42)}px`);
</script>

<template>
  <span
    class="flex shrink-0 items-center justify-center overflow-hidden bg-hover text-fg-3"
    :style="boxStyle"
  >
    <img
      v-if="showImage"
      :src="props.src!"
      :alt="props.name"
      class="size-full object-contain"
      draggable="false"
      @error="failed = true"
    />
    <span
      v-else
      class="flex size-full items-center justify-center bg-hover font-semibold text-fg-3"
      :style="{ fontSize }"
    >
      {{ initial(props.name) }}
    </span>
  </span>
</template>
