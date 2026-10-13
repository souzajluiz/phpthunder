<?php

namespace App\Http;

use App\Models\Post;

class PostController
{
    /**
     * @param Post $post
     */
    public function show($post): void
    {
        $post->author->getName();
        foreach ($post->approved() as $comment) {
            $comment->writer()->greet();
        }
        $first = $post->comments[0];
        $first->body();
        /** @var Post $draft */
        $draft = cache('draft');
        $draft->approved();
        $found = Post::published()->first();
        $found->comments;
    }
}
