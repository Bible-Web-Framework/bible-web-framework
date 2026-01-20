<script lang="ts" setup>
import type { SearchResponse } from '~/bwfApi'

const config = useRuntimeConfig()
const route = useRoute()
const query = ref((route.query.q || '').toString())
const loadingIndicator = useLoadingIndicator()
const { data: searchResults } = await useFetch<SearchResponse>('/v1/search', {
  baseURL: config.public.apiRootUrl,
  query: {
    term: query,
  },
  onRequest: () => loadingIndicator.start(),
  onRequestError: () => loadingIndicator.finish({ error: true }),
  onResponse: () => loadingIndicator.finish(),
  onResponseError: () => loadingIndicator.finish({ error: true }),
})

const newQuery = ref(query.value)
function search() {
  const url = new URL(window.location.href)
  url.searchParams.set('q', newQuery.value)
  window.history.pushState(null, '', url)
  query.value = newQuery.value
}
</script>

<template>
  <div>
    <h1>Search Page</h1>
    <input v-model="newQuery" placeholder="Enter search term" @keyup.enter="search" />
    <button @click="search">Search</button>

    <div v-if="searchResults">
      <template v-if="searchResults.response_type === 'scripture_passages'">
        <template
          v-for="(reference, referenceIndex) in searchResults.references"
          :key="referenceIndex"
        >
          <hr v-if="referenceIndex > 0" />
          <template v-if="'content' in reference">
            <UsjContentsRenderer v-if="reference.content" :contents="reference.content" />
          </template>
        </template>
      </template>
      <template v-else-if="searchResults.search_term">
        <h2>
          {{ searchResults.references.length }} results found for '{{ searchResults.search_term }}':
        </h2>
        <table>
          <tr v-for="(reference, referenceIndex) in searchResults.references" :key="referenceIndex">
            <td v-if="'invalid_reference' in reference">{{ reference.details }}</td>
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
