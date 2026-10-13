<?php

namespace App\Models;

use App\Support\Builder;

/**
 * A blog post.
 *
 * @property-read User $author
 * @method static Builder published()
 */
class Post extends Model
{
    /** @var Comment[] */
    public $comments = [];

    /**
     * Comments that passed moderation.
     *
     * @return Comment[]
     */
    public function approved()
    {
        return $this->comments;
    }
}
