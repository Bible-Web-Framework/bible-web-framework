<script lang="ts" setup>
import type { FunctionalComponent, VNode } from 'vue'
import type { ApiV1, SearchResponseResult } from '~/bwfApi'
import UsjContentsRenderer from '~/components/UsjContentsRenderer.vue'
import { normalizeNoteCallers, walkUsj, type ParaContent } from '~/usj'

const config = useRuntimeConfig()
const { data: biblesData } = await useFetch<ApiV1['bibles']>('/v1/bibles', {
  baseURL: config.public.apiRootUrl,
})

const route = useRoute()
const router = useRouter()

const bible = computed({
  get: () => (route.query.bible || biblesData.value?.default_bible || '').toString(),
  set: (bible) => router.push({ query: { ...route.query, bible } }),
})
const query = computed({
  get: () => (route.query.q || '').toString(),
  set: (q) => router.push({ query: { ...route.query, q } }),
})
const page = computed({
  get: () => Math.max(Math.round(+(route.query.page || '1').toString() || 1), 1),
  set: (page) => router.push({ query: { ...route.query, page } }),
})
const resultsPerPage = computed({
  get: () => Math.min(Math.max(Math.round(+(route.query.count || '50').toString() || 50), 1), 250),
  set: (count) => router.push({ query: { ...route.query, count } }),
})

const loadingIndicator = useLoadingIndicator()
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
      onRequest: () => loadingIndicator.start(),
      onRequestError: () => loadingIndicator.finish({ error: true }),
      onResponse: () => {
        loadingIndicator.finish()
      },
      onResponseError: () => loadingIndicator.finish({ error: true }),
    })

    let noteId = 0
    for (const reference of response.references) {
      if ('content' in reference && reference.content) {
        noteId = normalizeNoteCallers(reference.content, noteId)
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
function search() {
  query.value = newQuery.value
}

function gotoSearchResult(result: SearchResponseResult) {
  if ('invalid_reference' in result) {
    return
  }
  newQuery.value = `${result.translated_book_name} ${result.reference.chapter}:${result.reference.verses}`
  search()
}

watch(query, () => (newQuery.value = query.value))
watch(resultsPerPage, (newCount, oldCount) => {
  const oldStart = (page.value - 1) * oldCount
  page.value = Math.floor(oldStart / newCount) + 1
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
            class: 'f',
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

    <div>
      <select v-if="biblesData" v-model="bible">
        <option v-for="(info, id) in biblesData.bibles" :key="id" :value="id">
          {{ info.display_name ?? id.toLocaleUpperCase() }}
        </option>
      </select>
    </div>
    <div>
      <input v-model="newQuery" placeholder="Enter search term" @keyup.enter="search" />
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
            <UsjContentsRenderer v-if="reference.content" :contents="reference.content" />
            <p v-else class="error">
              No scripture passage found for {{ reference.translated_book_name }}
              {{ reference.reference.chapter }}:{{ reference.reference.verses }}
            </p>
          </template>
          <td v-else class="error">{{ reference.details }}</td>
        </template>
        <template v-if="searchData?.noteCount">
          <hr />
          <template
            v-for="(reference, referenceIndex) in searchResults.references"
            :key="referenceIndex"
          >
            <NotesRenderer
              v-if="'content' in reference && reference.content"
              :contents="reference.content"
            />
          </template>
        </template>
      </template>
      <template v-else-if="searchResults.search_term">
        <h2>
          {{ searchResults.total_results }} results found for '{{ searchResults.search_term }}':
        </h2>
        <table>
          <tr
            v-for="(reference, referenceIndex) in searchResults.references"
            :key="referenceIndex"
            @click="gotoSearchResult(reference)"
          >
            <td v-if="'invalid_reference' in reference" class="error" colspan="2">
              {{ reference.details }}
            </td>
            <template v-else>
              <td>
                {{ reference.translated_book_name }} {{ reference.reference.chapter }}:{{
                  reference.reference.verses
                }}
              </td>
              <td v-if="reference.content">
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
