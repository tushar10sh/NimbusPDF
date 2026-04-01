This project features following capabilities:
1. a PDF document viewer with zoom in, zoom out, continous page viewing, keyword search in the document, Highlighting sections and saving those highlights in a markdown file for future reference and reopening of the document. The user should also be able to write notes for each page of the document and that needs to be saved in a separate markdown file.
2. an offline AI helper to guide user about their queries regarding document content. The AI assistant opens in sidebar and maintains chat history with respect to each pdf in a separate markdown file. 
3. The whole application is web based and has optional user login via openid to enable users to upload and save pdf for future use. 
4. The AI assistant has OpenAI/Ollama style integrations and user can configure their own endpoints of similar nature for using their local models. 
5. The whole solution is containerized for deployment so that user can run the service locally on their computer and be in control of their own data.
6. The AI assistant should also offer quick tools like document summary, points to remember. 
7. In the Authenticated mode, with each pdf upload, ask a question to user "Do you want to add this document's knowledge in the Long term memory?". If the user answers yes, generate the docucment summary and key points and integrate those into long term user memory. The user also has control over the long term memory document, which he can edit inside the application as well.
8. In the Authenticated mode, a user should be able to provide categorization of his documents like grouping simlilar document together to build a better knowledge graph. Keep options to integrate graph RAG later in the project. 
9. All the user data has to organized in simple folders so that he may download it any time 
