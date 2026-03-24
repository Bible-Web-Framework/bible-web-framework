<script setup lang="ts">
import arrayEqual from 'array-equal'
import type { FunctionalComponent } from 'vue'
import type { LocationQueryRaw } from 'vue-router'
import type { HighlightsArray } from '~/bwfApi'
import { MACHINE_REFERENCE_REGEX, type ParaContent } from '~/usj'

const props = withDefaults(
  defineProps<{
    contents: ParaContent[]
    highlights?: HighlightsArray
    ignoredContentTypes?: string[]
    generateSearchQuery?: (q: string, normalize: boolean) => LocationQueryRaw
    currentPath?: number[]
  }>(),
  {
    highlights: () => [],
    ignoredContentTypes: () => [],
    generateSearchQuery: undefined,
    currentPath: () => [],
  },
)

const RenderWithHighlight: FunctionalComponent<{ text: string; textIndex: number }> = ({
  text,
  textIndex,
}) => {
  const path = props.currentPath.concat(textIndex)
  const highlights = props.highlights.filter(
    (range) => range.start.usj_path <= path && path <= range.end.usj_path,
  )
  if (!highlights.length) {
    return text
  }
  const startHighlight = highlights.find((x) => arrayEqual(x.start.usj_path, path))?.start?.char
  const endHighlight = highlights.find((x) => arrayEqual(x.end.usj_path, path))?.end?.char

  const result = []
  if (startHighlight !== undefined && startHighlight > 0) {
    result.push(text.substring(0, startHighlight))
  }
  result.push(
    h('span', { class: 'usj-content search-highlight' }, [
      text.substring(startHighlight ?? 0, endHighlight),
    ]),
  )
  if (endHighlight !== undefined && endHighlight < text.length) {
    result.push(text.substring(endHighlight))
  }
  return result
}
RenderWithHighlight.props = {
  text: {
    type: String,
    required: true,
  },
  textIndex: {
    type: Number,
    required: true,
  },
}

const UnimplementedMarker: FunctionalComponent<{ marker: string }> = import.meta.dev
  ? ({ marker }) => h('code', [`[Unimplemented marker/type: ${marker}]`])
  : () => {}
UnimplementedMarker.props = {
  marker: {
    type: String,
    required: true,
  },
}
</script>

<template>
  <template v-for="(content, contentIndex) in contents" :key="contentIndex">
    <RenderWithHighlight
      v-if="typeof content === 'string'"
      :text="content"
      :text-index="contentIndex"
    />
    <template v-else-if="ignoredContentTypes.includes(content.type)"></template>
    <!-- TODO: Support \ca and \va when https://github.com/jcuenod/usfm3/issues/2 is fixed -->
    <span v-else-if="content.type === 'chapter'" class="usj-content c">{{
      content.pubnumber ?? content.number
    }}</span>
    <span
      v-else-if="content.type === 'verse'"
      class="usj-content v"
      :data-verse-1="(content.pubnumber ?? content.number) === '1' ? true : undefined"
      >{{ content.pubnumber ?? content.number }}</span
    >
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
          :generate-search-query="generateSearchQuery"
          :current-path="currentPath.concat(contentIndex)"
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
          :generate-search-query="generateSearchQuery"
          :current-path="currentPath.concat(contentIndex)"
        />
      </p>
      <br
        v-else-if="['nb', 'b'].includes(content.marker)"
        :class="['usj-content', content.marker]"
      />
      <UnimplementedMarker v-else :marker="content.marker" />
    </template>
    <template v-else-if="content.type === 'char'">
      <!-- TODO: Implement \ref -->
      <span
        v-if="
          [
            'add',
            'bk',
            'dc',
            'em',
            'k',
            'nd',
            'ord',
            'pn',
            'png',
            'qt',
            'rq',
            // TODO: Implement ref, sig, sls, tl, w, wa, wg, and wh
            'wj',
            'fr',
            'ft',
          ].includes(content.marker)
        "
        :class="['usj-content', content.marker]"
      >
        <UsjContentsRenderer
          :contents="content.content"
          :highlights="highlights"
          :ignored-content-types="ignoredContentTypes"
          :generate-search-query="generateSearchQuery"
          :current-path="currentPath.concat(contentIndex)"
      /></span>
      <NuxtLink
        v-else-if="content.marker === 'jmp'"
        :id="content.id"
        :href="
          (content.href &&
            MACHINE_REFERENCE_REGEX.test(content.href) &&
            generateSearchQuery?.(content.href, true)) ||
          content.href
        "
        :title="content.title"
        class="usj-content jmp"
        ><UsjContentsRenderer
          :contents="content.content"
          :highlights="highlights"
          :ignored-content-types="ignoredContentTypes"
          :generate-search-query="generateSearchQuery"
          :current-path="currentPath.concat(contentIndex)"
      /></NuxtLink>
      <ruby v-else-if="content.marker === 'rb'" class="usj-content rb"
        ><UsjContentsRenderer
          :contents="content.content"
          :highlights="highlights"
          :ignored-content-types="ignoredContentTypes"
          :generate-search-query="generateSearchQuery"
          :current-path="currentPath.concat(contentIndex)"
        /><rp>(</rp><rt>{{ content.gloss }}</rt
        ><rp>)</rp></ruby
      >
      <UnimplementedMarker v-else :marker="content.marker" />
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
      <UnimplementedMarker v-else :marker="content.marker" />
    </template>
    <UnimplementedMarker v-else :marker="'marker' in content ? content.marker : content.type" />
  </template>
</template>

<style>
@import url('~/assets/usj.css');
</style>
