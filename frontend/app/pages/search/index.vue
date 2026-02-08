<script lang="ts" setup>
import type { BibleSearchResponse } from '~/bwfApi'

const config = useRuntimeConfig()
const route = useRoute()
const query = ref((route.query.q || '').toString())
const page = ref(Math.max(Math.round(+(route.query.page || '1').toString() || 1), 1))
const resultsPerPage = ref(
  Math.min(Math.max(Math.round(+(route.query.count || '50').toString() || 50), 1), 250),
)
const loadingIndicator = useLoadingIndicator()
const { data: searchResults } = await useFetch<BibleSearchResponse>('/v1/search', {
  baseURL: config.public.apiRootUrl,
  query: {
    term: query,
    start: computed(() => (page.value - 1) * resultsPerPage.value),
    count: resultsPerPage,
  },
  onRequest: () => loadingIndicator.start(),
  onRequestError: () => loadingIndicator.finish({ error: true }),
  onResponse: () => loadingIndicator.finish(),
  onResponseError: () => loadingIndicator.finish({ error: true }),
})

const pageCount = computed(() => {
  if (!searchResults.value || searchResults.value.response_type !== 'search_results') {
    return 0
  }
  return Math.ceil(searchResults.value.total_results / resultsPerPage.value)
})

function setQueryParam(name: string, value: string) {
  const url = new URL(window.location.href)
  url.searchParams.set(name, value)
  window.history.pushState(null, '', url)
}

const newQuery = ref(query.value)
function search() {
  setQueryParam('q', newQuery.value)
  query.value = newQuery.value
}

watch(page, () => setQueryParam('page', page.value.toString()))
watch(resultsPerPage, (newCount, oldCount) => {
  setQueryParam('count', newCount.toString())
  const oldStart = (page.value - 1) * oldCount
  page.value = Math.floor(oldStart / newCount) + 1
})
</script>

<template>
  <div>
    <h1>Search Page</h1>

    <input v-model="newQuery" placeholder="Enter search term" @keyup.enter="search" />
    <button @click="search">Search</button>

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
                {{ reference.translated_book_name }} {{ reference.reference.chapter }}:{{
                  reference.reference.verses
                }}
              </td>
              <td v-if="reference.content">
                <UsjContentsRenderer
                  :contents="reference.content"
                  :highlights="reference.highlights"
                />
              </td>
            </template>
          </tr>
        </table>
      </template>
    </div>
  </div>
</template>
