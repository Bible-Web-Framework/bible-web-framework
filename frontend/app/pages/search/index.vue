<script lang="ts" setup>
import type { FunctionalComponent, VNode } from 'vue'
import type { LocationQuery } from 'vue-router'
import { formatBibleReference, type ApiV1 } from '~/bwfApi'
import UsjContentsRenderer from '~/components/UsjContentsRenderer.vue'
import { normalizeNoteCallers, walkUsj, type ParaContent, type UsjContent } from '~/usj'

const config = useRuntimeConfig()
const { data: biblesData } = await useFetch<ApiV1['bibles']>('/v1/bibles', {
  baseURL: config.public.apiRootUrl,
})

const route = useRoute()
const router = useRouter()

const bible = computed(() =>
  (route.query.bible || biblesData.value?.default_bible || '').toString(),
)
const query = computed(() => (route.query.q || '').toString())
const page = computed({
  get: () => Math.max(Math.round(+(route.query.page || '1').toString() || 1), 1),
  set: (page) => router.push({ query: { ...route.query, page } }),
})
const resultsPerPage = computed({
  get: () => Math.min(Math.max(Math.round(+(route.query.count || '50').toString() || 50), 1), 250),
  set: (count) => {
    const oldCount = resultsPerPage.value
    const oldStart = (page.value - 1) * oldCount
    router.push({ query: { ...route.query, page: Math.floor(oldStart / count) + 1, count } })
  },
})

const { data: searchData } = await useAsyncData(
  'searchResults',
  async (_nuxtApp, { signal }) => {
    const response = await $fetch<ApiV1['bible']['search']>(`/v1/bible/${bible.value}/search`, {
      baseURL: config.public.apiRootUrl,
      query: {
        term: query.value,
        start: (page.value - 1) * resultsPerPage.value,
        count: resultsPerPage.value,
      },
      signal,
    })

    let noteId = 0
    for (const reference of response.references) {
      if ('content' in reference && reference.content) {
        noteId = normalizeNoteCallers(reference.content, noteId)
      }
    }

    for (const reference of response.references) {
      if (!('content' in reference) || !reference.content) {
        continue
      }
      const chapterIndex = reference.content.findIndex((el) => el.type === 'chapter')
      const chapter = reference.content[chapterIndex]!
      if (chapterIndex === -1 || chapter.type !== 'chapter') {
        continue
      }
      const isCl = (el: UsjContent) => el.type === 'para' && el.marker === 'cl'
      const clIndexBefore = reference.content.findIndex((el, i) => i < chapterIndex && isCl(el))
      const clIndexAfter = reference.content.findIndex((el, i) => i > chapterIndex && isCl(el))
      if (clIndexAfter !== -1) {
        const cl = reference.content.splice(clIndexAfter, 1)[0]!
        if (clIndexBefore !== -1) {
          reference.content.splice(clIndexBefore, 1, cl)
        } else {
          reference.content.splice(0, 0, cl)
        }
      } else if (clIndexBefore !== -1) {
        const cl = reference.content[clIndexBefore]!
        if (cl.type !== 'para') {
          continue
        }
        cl.content = (cl.content ?? []).concat(` ${chapter.pubnumber ?? chapter.number}`)
      } else {
        reference.content.splice(0, 0, {
          type: 'para',
          marker: 'cl',
          content: [`${reference.translated_book_name} ${chapter.pubnumber ?? chapter.number}`],
        })
      }
    }

    return {
      results: response,
      noteCount: noteId,
    }
  },
  {
    watch: [bible, query, page, resultsPerPage],
  },
)
const searchResults = computed(() => searchData.value?.results)

const pageCount = computed(() => {
  if (!searchResults.value || searchResults.value.response_type !== 'search_results') {
    return 0
  }
  return Math.ceil(searchResults.value.total_results / resultsPerPage.value)
})

const newQuery = ref(query.value)
const newBible = ref(bible.value)
function newQueryParamsForSearch(q: string, bible: string) {
  const newQueryParams: LocationQuery = { ...route.query, q, bible }
  delete newQueryParams['page']
  return newQueryParams
}
function search() {
  router.push({ query: newQueryParamsForSearch(newQuery.value, newBible.value) })
}

watch([query, bible], () => {
  newQuery.value = query.value
  newBible.value = bible.value
})

const NotesRenderer: FunctionalComponent<{ contents: ParaContent[] }> = ({ contents }) => {
  const notes: VNode[] = []
  walkUsj(contents, (element) => {
    if (typeof element === 'string' || element.type !== 'note') {
      return true
    }
    notes.push(
      h('div', { class: 'note-contents' }, [
        h(
          'a',
          {
            class: 'usj-content f',
            name: `note-contents-${element.caller}`,
            href: `#note-source-${element.caller}`,
          },
          [element.caller],
        ),
        h(UsjContentsRenderer, { contents: element.content, ignoredContentTypes: ['note'] }),
      ]),
    )
    return false
  })
  return notes
}
</script>

<template>
  <div>
    <h1>Search Page</h1>

    <div class="search-line">
      <input v-model="newQuery" placeholder="Enter search term" @keyup.enter="search" />
      <select v-if="biblesData" v-model="newBible" class="padded-bible-select">
        <option v-for="(info, id) in biblesData.bibles" :key="id" :value="id">
          {{ info.display_name ?? id.toLocaleUpperCase() }}
        </option>
      </select>
      <button @click="search">Search</button>
    </div>

    <template v-if="searchResults?.response_type === 'search_results'">
      <template v-if="pageCount > 1">
        Page:
        <select v-model="page">
          <option v-for="number in pageCount" :key="number" :value="number">
            {{ number }}
          </option>
        </select>
      </template>
      Results per page:
      <select v-model="resultsPerPage">
        <option :value="50">50</option>
        <option :value="100">100</option>
        <option :value="150">150</option>
        <option :value="200">200</option>
        <option :value="250">250</option>
      </select>
    </template>

    <div v-if="searchResults">
      <template v-if="searchResults.response_type === 'scripture_passages'">
        <template
          v-for="(reference, referenceIndex) in searchResults.references"
          :key="referenceIndex"
        >
          <hr v-if="referenceIndex > 0" />
          <template v-if="'content' in reference">
            <div v-if="reference.content" class="usj-container">
              <UsjContentsRenderer :contents="reference.content" />
            </div>
            <p v-else class="error">
              No scripture passage found for {{ formatBibleReference(reference) }}
            </p>
          </template>
          <td v-else class="error">{{ reference.details }}</td>
        </template>
        <template v-if="searchData?.noteCount">
          <hr />
          <div
            v-for="(reference, referenceIndex) in searchResults.references"
            :key="referenceIndex"
            class="usj-container"
          >
            <NotesRenderer
              v-if="'content' in reference && reference.content"
              :contents="reference.content"
            />
          </div>
        </template>
      </template>
      <template v-else-if="searchResults.search_term">
        <h2>
          {{ searchResults.total_results }} results found for '{{ searchResults.search_term }}':
        </h2>
        <table>
          <tr v-for="(reference, referenceIndex) in searchResults.references" :key="referenceIndex">
            <td v-if="'invalid_reference' in reference" class="error" colspan="2">
              {{ reference.details }}
            </td>
            <template v-else>
              <td>
                <NuxtLink
                  :to="{
                    query: newQueryParamsForSearch(formatBibleReference(reference), bible),
                  }"
                  >{{ formatBibleReference(reference) }}</NuxtLink
                >
              </td>
              <td v-if="reference.content" class="usj-container">
                <UsjContentsRenderer
                  :contents="reference.content"
                  :highlights="reference.highlights"
                  :ignored-content-types="['note', 'chapter', 'verse']"
                />
              </td>
            </template>
          </tr>
        </table>
      </template>
    </div>
  </div>
</template>

<style scoped>
@import url('https://fonts.googleapis.com/css2?family=Inter:ital,opsz,wght@0,14..32,100..900;1,14..32,100..900&display=swap');

.search-line {
  margin-block-end: 0.5em;
}

.padded-bible-select {
  margin-inline: 0.5em;
}

.usj-container {
  font-family: 'Inter', 'sans-serif';
  font-size: 18px;
}
</style>
