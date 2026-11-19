<script setup lang="ts">
withDefaults(
  defineProps<{
    modelValue: string;
    placeholder?: string;
    type?: string;
    autofocus?: boolean;
  }>(),
  { placeholder: "", type: "text", autofocus: false },
);

defineEmits<{ "update:modelValue": [value: string] }>();
</script>

<template>
  <input
    :type="type"
    :value="modelValue"
    :placeholder="placeholder"
    :autofocus="autofocus"
    autocomplete="off"
    autocorrect="off"
    autocapitalize="off"
    spellcheck="false"
    class="h-8 w-full rounded-md border border-line-strong bg-bg px-2.5 text-[13px] text-fg outline-none transition-colors placeholder:text-fg-3 focus:border-accent focus:ring-1 focus:ring-accent disabled:opacity-50"
    @input="($event.target as HTMLInputElement) && $emit('update:modelValue', ($event.target as HTMLInputElement).value)"
  />
</template>
