<script setup lang="ts">
import type { FunctionalComponent } from 'vue'
import type { HighlightsMap } from '~/bwfApi'
import type { ParaContent } from '~/usj'

const props = defineProps<{
  contents: ParaContent[]
  highlights?: HighlightsMap
  ignoredContentTypes?: string[]
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
      h('span', { class: 'usj-content search-highlight' }, [
        text.substring(highlight.start, highlight.end),
      ]),
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
    <template v-else-if="ignoredContentTypes?.includes(content.type)"></template>
    <span v-else-if="content.type === 'chapter'" class="usj-content c">{{ content.number }}</span>
    <span v-else-if="content.type === 'verse'" class="usj-content v">{{ content.number }}</span>
    <template v-else-if="content.type === 'para'">
      <!-- TODO: Implement \ip when an example is found -->
      <p
        v-if="
          [
            'cl',
            'p',
            'm',
            'po',
            'cls',
            'pr',
            'pc',
            'pm',
            'pmo',
            'pmc',
            'pmr',
            'lit',
            'qr',
            'qc',
            'qa',
            'qd',
          ].includes(content.marker)
        "
        :class="{
          'usj-content': true,
          [content.marker]: true,
          poetry: content.marker.startsWith('q'),
          'poetry-block': ['qr', 'qc', 'qa'].includes(content.marker),
        }"
      >
        <UsjContentsRenderer
          v-if="content.content"
          :contents="content.content"
          :highlights="highlights"
          :ignored-content-types="ignoredContentTypes"
        />
      </p>
      <p
        v-else-if="/^([pm]i[1-3]?|q[1-4]?|qm[1-3]?)$/.test(content.marker)"
        :class="{
          'usj-content': true,
          [content.marker.replace(/\d/g, '')]: true,
          poetry: content.marker.startsWith('q'),
          'poetry-block': /^(q\d)$/.test(content.marker),
        }"
        :data-usj-indent="+content.marker.replace(/[^\d]/g, '') || 1"
      >
        <UsjContentsRenderer
          v-if="content.content"
          :contents="content.content"
          :highlights="highlights"
          :ignored-content-types="ignoredContentTypes"
        />
      </p>
      <!-- \nb is not implemented... do we even want to? -->
      <br v-else-if="content.marker === 'b'" class="usj-content b" />
    </template>
    <template v-else-if="content.type === 'char'">
      <span v-if="['fr', 'ft'].includes(content.marker)" :class="['usj-content', content.marker]">
        <UsjContentsRenderer
          :contents="content.content"
          :highlights="highlights"
          :ignored-content-types="ignoredContentTypes"
        />
      </span>
    </template>
    <!-- TODO: Implement Milestones -->
    <template v-else-if="content.type === 'note'">
      <a
        v-if="content.marker === 'f' && content.caller !== '-'"
        class="usj-content note-source f"
        :name="`note-source-${content.caller}`"
        :href="`#note-contents-${content.caller}`"
        >{{ content.caller }}</a
      >
    </template>
  </template>
</template>

<style>
@import url('~/assets/bwf.css');
</style>
