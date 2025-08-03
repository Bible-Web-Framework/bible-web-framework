# Bible Web Framework

Framework written in Rust to serve up bible content

Query will be: `/api/v1/scripture?term=<search-term>&languages=<lang1>,<lang2>`
Examples:
1. `/api/v1/scripture?term=Gabriel`
2. `/api/v1/scripture?term=Lk3:23-26;Mk1:14-45;2:1-17;3:1-15;Mt3:1-2&languages=ml,original`

Special "languages":
* `original` indicates Hebrew for OT and/or Greek for NT
* `septuagint` is similar, but Greek for OT and NT

## Response to #1 will look like:
```json
{
    "response_type": "search_results",
    "search_term": "Gabriel",
    "references": [
        {
            "reference": "Daniel 8:16",
            "book": "Daniel",
            "chapter": 8,
            "verse_range": [16, 16],
            "content": "Just the verse"
        },
        {
            "reference": "Daniel 9:20",
            "content": "Just the verse"
        },
        {
            "reference": "Daniel 9:21",
            "content": "Just the verse"
        },
        {
            "reference": "Luke 1:19",
            "content": "Just the verse"
        },
        {
            "reference": "Luke 1:26",
            "content": "Just the verse"
        }
    ]
}
```

## Response to #2 will look like:
```json
{
    "response_type": "scripture_passages",
    "references": [
        {
            "reference": "Luke 3:23-26",
            "book": "Luke",
            "chapter": 3,
            "verse_range": [23, 26],
            "content": [
                {
                    "type": "chapter",
                    "marker": "c",
                    "number": "3",
                    "sid": "LUK 3"
                },
                {
                    "type": "para",
                    "marker": "p",
                    "content": [
                        {
                            "type": "verse",
                            "marker": "v",
                            "number": "23",
                            "sid": "LUK 3:23"
                        },
                        "Isa himself, when he began to teach, was about thirty years old, being the son (as was supposed) of Yusuf, the son of\nHeli, \n",
                        {
                            "type": "verse",
                            "marker": "v",
                            "number": "24",
                            "sid": "LUK 3:24"
                        },
                        "the son of Matthat, the son of Levi, the son of Melchi, the son of Jannai, the son of Yusuf,\n",
                        {
                            "type": "verse",
                            "marker": "v",
                            "number": "25",
                            "sid": "LUK 3:25"
                        },
                        "the son of Mattathias, the son of Amos, the son of Nahum, the son of Esli, the son of Naggai,\n",
                        {
                            "type": "verse",
                            "marker": "v",
                            "number": "26",
                            "sid": "LUK 3:26"
                        },
                        "the son of Maath, the son of Mattathias, the son of Semein, the son of Yusuf, the son of Judah,\n"
                    ]
                }
            ]
        },
        {
            "reference": "Mark 1:14-45",
            "book": "Mark",
            "chapter": 1,
            "verse_range": [14, 45]
        }
    ]
}
```

How to parse this JSON is determined by finding the "sid", which consists of the official Paratext abbreviation of the book, space, chapter:verse

