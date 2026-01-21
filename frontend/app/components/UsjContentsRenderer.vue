<script setup lang="ts">
import type { FunctionalComponent } from 'vue'
import type { HighlightsMap } from '~/bwfApi'
import type { ParaContent } from '~/usj'

const props = defineProps<{
  contents: ParaContent[]
  highlights?: HighlightsMap
}>()

const RenderWithHighlight: FunctionalComponent<{ text: string; suffix?: string }> = ({
  text,
  suffix,
}) => {
  const highlights = props.highlights?.[text]
  if (suffix) {
    text += suffix
  }
  if (!highlights) {
    return text
  }
  const result = []
  let lastEnd = 0
  for (const highlight of highlights) {
    if (highlight.start > lastEnd) {
      result.push(text.substring(lastEnd, highlight.start))
    }
    result.push(h('b', {}, [text.substring(highlight.start, highlight.end)]))
    lastEnd = highlight.end
  }
  if (lastEnd < text.length) {
    result.push(text.substring(lastEnd))
  }
  return result
}
</script>

<template>
  <template v-for="(content, contentIndex) in contents" :key="contentIndex">
    <template v-if="typeof content === 'string'"
      ><RenderWithHighlight :text="content" suffix=" "
    /></template>
    <template v-else-if="content.type === 'para'">
      <p v-if="content.marker === 'p'">
        <UsjContentsRenderer
          v-if="content.content"
          :contents="content.content"
          :highlights="highlights"
        />
      </p>
    </template>
  </template>
</template>
