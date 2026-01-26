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
    result.push(
      h('span', { class: 'search-highlight' }, [text.substring(highlight.start, highlight.end)]),
    )
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
    <RenderWithHighlight v-if="typeof content === 'string'" :text="content" suffix=" " />
    <span v-else-if="content.type === 'chapter'" class="chapter-number">{{ content.number }}</span>
    <span v-else-if="content.type === 'verse'" class="verse-number">{{ content.number }}</span>
    <template v-else-if="content.type === 'para'">
      <!-- TODO: Implement \ip when an example is found -->
      <!-- TODO: Implement Titles and Sections -->
      <!-- #region Body Paragraphs -->
      <p v-if="content.marker === 'p' || (content.marker === 'm' && content.content)">
        <UsjContentsRenderer
          v-if="content.content"
          :contents="content.content"
          :highlights="highlights"
        />
      </p>
      <!-- #endregion -->
    </template>
  </template>
</template>

<style>
@import url('~/assets/bwf.css');
</style>
